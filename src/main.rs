use clap::{Parser, ValueEnum};
use regex::Regex;
use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde_json::{json, Map, Value};
use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::thread::sleep;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "humanize-text")]
#[command(version = "0.13.0")]
#[command(about = "Rewrite technical articles with OpenRouter or local cleanup rules")]
struct Args {
    /// Input text file
    input: String,

    /// Output text file
    output: String,

    /// Rewrite mode: light is local cleanup only, strong uses OpenRouter
    #[arg(long, value_enum, default_value_t = Mode::Light)]
    mode: Mode,

    /// Rewrite style. Use reference-article to follow the publication structure from the EDA reference article.
    #[arg(long, value_enum, default_value_t = RewriteStyle::ReferenceArticle)]
    style: RewriteStyle,

    /// Primary OpenRouter model
    #[arg(long, default_value = "google/gemini-2.5-flash")]
    model: String,

    /// Optional fallback models. Can be repeated.
    #[arg(long = "fallback-model")]
    fallback_models: Vec<String>,

    /// OpenRouter API key. Prefer OPENROUTER_API_KEY or .env in real usage.
    #[arg(long)]
    api_key: Option<String>,

    /// Reasoning effort for reasoning-capable models.
    #[arg(long, value_enum, default_value_t = ReasoningEffort::Auto)]
    reasoning_effort: ReasoningEffort,

    /// Max output tokens per OpenRouter request.
    #[arg(long, default_value_t = 5200)]
    max_tokens: u32,

    /// Max input characters per chunk. For reference-article style, keep this high so the article is rewritten in one pass.
    #[arg(long, default_value_t = 20000)]
    max_chunk_chars: usize,

    /// Sampling temperature.
    #[arg(long, default_value_t = 0.82)]
    temperature: f32,

    /// Frequency penalty.
    #[arg(long, default_value_t = 0.25)]
    frequency_penalty: f32,

    /// Presence penalty.
    #[arg(long, default_value_t = 0.15)]
    presence_penalty: f32,

    /// Number of 429 retries before moving to the next fallback model.
    #[arg(long, default_value_t = 1)]
    rate_limit_retries_before_fallback: u32,

    /// Optional app URL for OpenRouter attribution.
    #[arg(long, default_value = "https://github.com/local/humanize-text-rust")]
    http_referer: String,

    /// Optional app title for OpenRouter attribution.
    #[arg(long, default_value = "Humanize Text Rust")]
    app_title: String,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum Mode {
    Light,
    Strong,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum RewriteStyle {
    /// Clean grammar and flow without changing the structure much.
    Clean,
    /// Senior engineer tone; practical and clear.
    Practitioner,
    /// Field-note style; grounded and less corporate.
    FieldNote,
    /// Rebuild the article using a publication-style structure: takeaways, foundation, why it matters, what hurts, patterns, examples, conclusion.
    ReferenceArticle,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum ReasoningEffort {
    Auto,
    None,
    Low,
    Medium,
    High,
}

fn main() -> Result<(), Box<dyn Error>> {
    dotenvy::dotenv().ok();
    let args = Args::parse();

    let input_text = fs::read_to_string(&args.input)?;

    let output = match args.mode {
        Mode::Light => {
            eprintln!("Running local light cleanup. This mode does not restructure the article.");
            final_repair_pass(&apply_light_rewrites(&input_text))
        }
        Mode::Strong => rewrite_strong(&input_text, &args)?,
    };

    fs::write(&args.output, output.trim())?;
    println!("Done. Output written to {}", args.output);
    Ok(())
}

fn rewrite_strong(input_text: &str, args: &Args) -> Result<String, Box<dyn Error>> {
    let api_key = args
        .api_key
        .clone()
        .or_else(|| env::var("OPENROUTER_API_KEY").ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Missing OpenRouter API key. Set OPENROUTER_API_KEY, create a .env file, or pass --api-key."))?;

    let chunks = chunk_text(input_text, args.max_chunk_chars);

    if args.style == RewriteStyle::ReferenceArticle && chunks.len() > 1 {
        eprintln!(
            "Warning: reference-article style works best in one pass. Input was split into {} chunks. Increase --max-chunk-chars if your model context allows it.",
            chunks.len()
        );
    }

    let mut rewritten_chunks = Vec::new();

    for (idx, chunk) in chunks.iter().enumerate() {
        eprintln!("Rewriting chunk {}/{}...", idx + 1, chunks.len());
        let prompt = build_user_prompt(chunk, args.style, chunks.len() > 1, idx + 1, chunks.len());
        let raw = call_with_fallbacks(&prompt, args, &api_key)?;
        let repaired = final_repair_pass(&raw);
        rewritten_chunks.push(repaired);
    }

    let combined = rewritten_chunks.join("\n\n");
    Ok(final_repair_pass(&combined))
}

fn call_with_fallbacks(prompt: &str, args: &Args, api_key: &str) -> Result<String, Box<dyn Error>> {
    let mut models = Vec::new();
    models.push(args.model.clone());
    for fallback in &args.fallback_models {
        if !models.contains(fallback) {
            models.push(fallback.clone());
        }
    }

    let mut last_error = String::new();

    for model in models {
        eprintln!("Trying OpenRouter model '{}'...", model);
        match call_openrouter_model(prompt, args, api_key, &model) {
            Ok(text) => return Ok(text),
            Err(err) => {
                last_error = err.to_string();
                eprintln!("Model '{}' failed: {}", model, last_error);
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::Other,
        format!("All OpenRouter models failed. Last error: {}", last_error),
    )
    .into())
}

fn call_openrouter_model(
    prompt: &str,
    args: &Args,
    api_key: &str,
    model: &str,
) -> Result<String, Box<dyn Error>> {
    let client = Client::builder()
        .timeout(Duration::from_secs(180))
        .build()?;

    let mut attempts = 0;
    let mut force_low_reasoning = false;

    loop {
        let request_body = build_openrouter_body(prompt, args, model, force_low_reasoning);

        let response = client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .bearer_auth(api_key)
            .header("HTTP-Referer", &args.http_referer)
            .header("X-OpenRouter-Title", &args.app_title)
            .json(&request_body)
            .send()?;

        let status = response.status();
        let headers = response.headers().clone();
        let body_text = response.text()?;

        if status.is_success() {
            let json: Value = serde_json::from_str(&body_text)?;
            return extract_rewritten_content(&json);
        }

        if status == StatusCode::TOO_MANY_REQUESTS {
            if attempts < args.rate_limit_retries_before_fallback {
                attempts += 1;
                let retry_after = headers
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(30);

                eprintln!(
                    "OpenRouter rate limit reached for model '{}'. Retrying in {} seconds ({}/{})...",
                    model, retry_after, attempts, args.rate_limit_retries_before_fallback
                );
                sleep(Duration::from_secs(retry_after.min(60)));
                continue;
            }
        }

        if status == StatusCode::BAD_REQUEST
            && body_text.contains("Reasoning is mandatory")
            && !force_low_reasoning
        {
            eprintln!("Model requires reasoning. Retrying with reasoning effort 'low'.");
            force_low_reasoning = true;
            continue;
        }

        if status.as_u16() == 402 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "OpenRouter payment/credit error 402. Add credits, check the correct account/org, or use another model. Response: {}",
                    body_text
                ),
            )
            .into());
        }

        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "OpenRouter request failed with status {}. Response: {}",
                status, body_text
            ),
        )
        .into());
    }
}

fn build_openrouter_body(
    prompt: &str,
    args: &Args,
    model: &str,
    force_low_reasoning: bool,
) -> Value {
    let mut body = Map::new();
    body.insert("model".to_string(), json!(model));
    body.insert("temperature".to_string(), json!(args.temperature));
    body.insert("max_tokens".to_string(), json!(args.max_tokens));
    body.insert("frequency_penalty".to_string(), json!(args.frequency_penalty));
    body.insert("presence_penalty".to_string(), json!(args.presence_penalty));

    let system_prompt = build_system_prompt(args.style);
    body.insert(
        "messages".to_string(),
        json!([
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": prompt}
        ]),
    );

    if let Some(reasoning_value) = reasoning_json(model, args.reasoning_effort, force_low_reasoning) {
        body.insert("reasoning".to_string(), reasoning_value);
    }

    Value::Object(body)
}

fn reasoning_json(model: &str, effort: ReasoningEffort, force_low: bool) -> Option<Value> {
    if force_low {
        return Some(json!({"effort": "low", "exclude": true}));
    }

    let is_gpt_oss = model.contains("gpt-oss");

    match effort {
        ReasoningEffort::Auto => {
            if is_gpt_oss {
                Some(json!({"effort": "low", "exclude": true}))
            } else {
                None
            }
        }
        ReasoningEffort::None => {
            if is_gpt_oss {
                Some(json!({"effort": "low", "exclude": true}))
            } else {
                Some(json!({"effort": "none", "exclude": true}))
            }
        }
        ReasoningEffort::Low => Some(json!({"effort": "low", "exclude": true})),
        ReasoningEffort::Medium => Some(json!({"effort": "medium", "exclude": true})),
        ReasoningEffort::High => Some(json!({"effort": "high", "exclude": true})),
    }
}

fn extract_rewritten_content(json: &Value) -> Result<String, Box<dyn Error>> {
    let choice = json
        .get("choices")
        .and_then(|c| c.get(0))
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "OpenRouter response had no choices"))?;

    let finish_reason = choice
        .get("finish_reason")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let content_value = choice
        .get("message")
        .and_then(|m| m.get("content"))
        .unwrap_or(&Value::Null);

    let content = match content_value {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| {
                if let Some(text) = p.get("text").and_then(|t| t.as_str()) {
                    Some(text.to_string())
                } else if let Some(text) = p.as_str() {
                    Some(text.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<String>>()
            .join("\n"),
        _ => String::new(),
    };

    let reasoning_tokens = json
        .pointer("/usage/completion_tokens_details/reasoning_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    if content.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "OpenRouter returned no final rewritten text. finish_reason={}, reasoning_tokens={}. Increase --max-tokens, use a non-reasoning model, or use --reasoning-effort low/none as appropriate.",
                finish_reason, reasoning_tokens
            ),
        )
        .into());
    }

    if finish_reason == "length" {
        eprintln!("Warning: model stopped due to length. Output may be incomplete. Increase --max-tokens.");
    }

    Ok(content)
}

fn build_system_prompt(style: RewriteStyle) -> String {
    match style {
        RewriteStyle::Clean => {
            "You are a careful technical editor. Rewrite for clarity, grammar, and natural flow while preserving the original structure and meaning. Return only the rewritten text.".to_string()
        }
        RewriteStyle::Practitioner => {
            "You are a senior software engineering practitioner editing a technical article for a developer publication. Keep the writing practical, specific, and natural. Avoid corporate polish and casual slang. Return only the rewritten text.".to_string()
        }
        RewriteStyle::FieldNote => {
            "You are editing a field note from an experienced engineer. Make it sound grounded in real software delivery experience. Use clear prose, not marketing language. Do not add unsupported claims. Return only the rewritten text.".to_string()
        }
        RewriteStyle::ReferenceArticle => {
            "You are a senior technical editor preparing a developer-publication article. Rebuild the user's draft using a strong architecture-article structure similar to high-quality InfoQ-style technical writing. The output should feel human-edited, technically grounded, and publication-ready. Do not copy wording from any reference article. Do not add unsupported claims. Return only the rewritten article text.".to_string()
        }
    }
}

fn build_user_prompt(
    text: &str,
    style: RewriteStyle,
    is_chunked: bool,
    chunk_no: usize,
    chunk_total: usize,
) -> String {
    let chunk_note = if is_chunked {
        format!(
            "This is chunk {}/{} of a longer article. Rewrite only this chunk, but keep it compatible with the full article.\n\n",
            chunk_no, chunk_total
        )
    } else {
        String::new()
    };

    let style_instructions = match style {
        RewriteStyle::Clean => {
            "Clean up the text. Fix grammar, remove awkward phrasing, improve flow, and preserve the original structure."
        }
        RewriteStyle::Practitioner => {
            "Rewrite in a practical senior-engineer voice. Preserve technical accuracy. Remove AI-polished phrases. Do not make it casual or marketing-like."
        }
        RewriteStyle::FieldNote => {
            "Rewrite as a grounded field article. Use natural paragraph flow, clear transitions, and practitioner wording. Avoid numbered textbook structure unless the source already uses it."
        }
        RewriteStyle::ReferenceArticle => {
            "Restructure this draft using the following publication pattern:\n\n1. Key Takeaways: 4-5 concise bullets.\n2. Why This Topic Matters: explain the problem and why AI-assisted delivery changes the risk profile.\n3. What Loop Engineering Means: define the concept clearly.\n4. AI-Assisted Development as the Constraint: explain why generation speed creates a need for validation and evidence.\n5. Why Loop Engineering Matters in Practice: explain benefits with concrete engineering language.\n6. What Hurts — and What Actually Helps: describe common failure modes and the practices that reduce them.\n7. The Feedback Loops: cover intent, generation, validation, human review, runtime feedback, and business outcome. Do not make this a dry numbered textbook list; use flowing article prose.\n8. Example: AI-Assisted UI Generation.\n9. Example: Modernization.\n10. Bringing It All Together.\n11. Conclusion.\n\nImportant writing rules:\n- Keep the article human, clear, and professional.\n- Do not over-polish. Avoid words like invaluable, robust, seamless, transformative, crucial, paramount, indispensable, leverage, utilize, landscape, empowers, and cutting-edge.\n- Do not introduce fake data, fake quotes, fake company names, or unsupported claims.\n- Preserve important terms such as Agile, DevOps, CI/CD, DORA metrics, Spring Boot, API, UI, Maven, Gradle, MQ, Kafka, security, governance, and architecture.\n- Prefer concrete engineering wording over generic statements.\n- Vary sentence length.\n- Return only the rewritten article."
        }
    };

    format!(
        "{}{}\n\nSOURCE TEXT:\n{}",
        chunk_note, style_instructions, text
    )
}

fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
    if text.len() <= max_chars {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    for paragraph in text.split("\n\n") {
        let paragraph_with_break = if current.is_empty() {
            paragraph.to_string()
        } else {
            format!("\n\n{}", paragraph)
        };

        if current.len() + paragraph_with_break.len() > max_chars && !current.is_empty() {
            chunks.push(current.trim().to_string());
            current.clear();
        }

        if paragraph_with_break.len() > max_chars {
            for sentence in split_into_sentences(&paragraph_with_break) {
                if current.len() + sentence.len() + 1 > max_chars && !current.is_empty() {
                    chunks.push(current.trim().to_string());
                    current.clear();
                }
                current.push_str(&sentence);
                current.push(' ');
            }
        } else {
            current.push_str(&paragraph_with_break);
        }
    }

    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }

    chunks
}

fn apply_light_rewrites(text: &str) -> String {
    let mut result = text.to_string();

    let replacements = [
        ("It is important to note that ", ""),
        ("It should be noted that ", ""),
        ("It is worth noting that ", ""),
        ("In today's rapidly evolving digital landscape, ", ""),
        ("In today’s rapidly evolving digital landscape, ", ""),
        ("plays a crucial role in", "helps"),
        ("plays a vital role in", "helps"),
        ("a wide range of", "many"),
        ("in order to", "to"),
        ("due to the fact that", "because"),
        ("utilize", "use"),
        ("Utilize", "Use"),
        ("leverage", "use"),
        ("Leverage", "Use"),
        ("facilitates", "helps"),
        ("Facilitates", "Helps"),
        ("demonstrates", "shows"),
        ("Demonstrates", "Shows"),
        ("numerous", "many"),
        ("Numerous", "Many"),
    ];

    for (from, to) in replacements {
        result = result.replace(from, to);
    }

    final_repair_pass(&result)
}

fn final_repair_pass(text: &str) -> String {
    let mut result = text.to_string();

    let replacements = [
        ("proves invaluable", "is useful"),
        ("proved invaluable", "was useful"),
        ("paramount", "important"),
        ("indispensable", "important"),
        ("robust feedback mechanisms", "strong feedback loops"),
        ("robust feedback loop", "strong feedback loop"),
        ("strongness of their feedback loops", "strength of their feedback loops"),
        ("reviewing importantly", "reviewing carefully"),
        ("necessitates tests", "leads to tests"),
        ("mere compilation", "checking that it compiles"),
        ("mandates frequent", "encourages frequent"),
        ("the what business problem", "what business problem"),
        ("with out existing architecture", "with our existing architecture"),
        ("a important", "an important"),
        ("AI based", "AI-assisted"),
        ("AI-based", "AI-assisted"),
        ("crank out", "generate"),
        ("nail down", "define"),
        ("crystal clear", "clear"),
        ("the trick here", "the goal"),
        ("real win", "main benefit"),
        ("churning out", "generating"),
        ("production action", "production behavior"),
        ("doesn't replacing", "does not replace"),
        ("doesn’t replacing", "does not replace"),
        ("supposed to fill", "meant to address"),
    ];

    for (from, to) in replacements {
        result = result.replace(from, to);
    }

    // Remove common model wrappers.
    let wrapper_patterns = [
        r"(?im)^Here is (the )?(rewritten|enhanced|updated).*?:\s*\n",
        r"(?im)^Certainly[,.!]?\s*",
        r"(?im)^Sure[,.!]?\s*",
    ];
    for pattern in wrapper_patterns {
        let re = Regex::new(pattern).unwrap();
        result = re.replace_all(&result, "").to_string();
    }

    // Normalize accidental numbered headings generated by the model if not desired.
    let numbered_heading = Regex::new(r"(?m)^###\s+\d+\.\s+").unwrap();
    result = numbered_heading.replace_all(&result, "### ").to_string();

    clean_spacing_preserve_paragraphs(&result)
}

fn clean_spacing_preserve_paragraphs(text: &str) -> String {
    let punctuation_space = Regex::new(r"\s+([,.!?;:])").unwrap();
    let multiple_spaces = Regex::new(r"[ ]{2,}").unwrap();
    let repeated_blank_lines = Regex::new(r"\n{3,}").unwrap();

    let cleaned = text
        .lines()
        .map(|line| {
            let line = punctuation_space.replace_all(line, "$1").to_string();
            multiple_spaces.replace_all(&line, " ").trim_end().to_string()
        })
        .collect::<Vec<String>>()
        .join("\n");

    repeated_blank_lines.replace_all(&cleaned, "\n\n").trim().to_string()
}

fn split_into_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        current.push(ch);
        if matches!(ch, '.' | '!' | '?') {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                sentences.push(trimmed.to_string());
            }
            current.clear();
        }
    }

    if !current.trim().is_empty() {
        sentences.push(current.trim().to_string());
    }

    sentences
}
