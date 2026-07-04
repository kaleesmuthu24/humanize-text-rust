use clap::{Parser, ValueEnum};
use regex::Regex;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::io;
use std::thread::sleep;
use std::time::Duration;

type AppResult<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Clone, ValueEnum)]
enum Mode {
    /// Local rule-based cleanup only. No API key required.
    Light,
    /// OpenRouter LLM rewrite. Requires OPENROUTER_API_KEY, .env, or --api-key.
    Strong,
}

#[derive(Parser, Debug)]
#[command(name = "humanize_text")]
#[command(version = "0.5.0")]
#[command(about = "Rewrite AI-generated text into clearer, natural professional writing.")]
struct Args {
    /// Input text file
    input: String,

    /// Output text file
    output: String,

    /// Rewrite mode: light = local rules, strong = OpenRouter API
    #[arg(long, value_enum, default_value_t = Mode::Light)]
    mode: Mode,

    /// OpenRouter model id. Examples: openrouter/free, openai/gpt-4o-mini, openai/gpt-oss-20b:free
    #[arg(long, default_value = "openrouter/free")]
    model: String,

    /// Optional fallback model. Can be passed more than once.
    #[arg(long = "fallback-model")]
    fallback_models: Vec<String>,

    /// OpenRouter API key. Prefer OPENROUTER_API_KEY or .env instead of this flag.
    #[arg(long)]
    api_key: Option<String>,

    /// Maximum output tokens per chunk.
    #[arg(long, default_value_t = 1800)]
    max_tokens: u32,

    /// Maximum input characters per chunk before splitting.
    #[arg(long, default_value_t = 2500)]
    max_chunk_chars: usize,

    /// Maximum retries for 429/rate-limit responses per model.
    #[arg(long, default_value_t = 3)]
    max_retries: u32,

    /// Model temperature.
    #[arg(long, default_value_t = 0.65)]
    temperature: f32,

    /// Preserve markdown headings and paragraph breaks.
    #[arg(long, default_value_t = true)]
    preserve_markdown: bool,
}

fn main() -> AppResult<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();

    let input_text = fs::read_to_string(&args.input).map_err(|e| {
        make_error(format!(
            "Failed to read input file '{}': {}",
            args.input, e
        ))
    })?;

    let output = match args.mode {
        Mode::Light => apply_light_rewrites(&input_text),
        Mode::Strong => rewrite_with_openrouter(&input_text, &args)?,
    };

    fs::write(&args.output, output).map_err(|e| {
        make_error(format!(
            "Failed to write output file '{}': {}",
            args.output, e
        ))
    })?;

    println!("Completed successfully.");
    println!("Output saved to: {}", args.output);
    Ok(())
}

fn rewrite_with_openrouter(text: &str, args: &Args) -> AppResult<String> {
    let api_key = args
        .api_key
        .clone()
        .or_else(|| env::var("OPENROUTER_API_KEY").ok())
        .ok_or_else(|| {
            make_error(
                "Missing OpenRouter API key. Set OPENROUTER_API_KEY, create a .env file, or pass --api-key.",
            )
        })?;

    let chunks = split_into_chunks(text, args.max_chunk_chars);
    let client = Client::builder()
        .timeout(Duration::from_secs(180))
        .build()?;

    let mut rewritten_chunks = Vec::new();
    let total = chunks.len();

    for (idx, chunk) in chunks.iter().enumerate() {
        println!(
            "Rewriting chunk {}/{} with OpenRouter model '{}'...",
            idx + 1,
            total,
            args.model
        );

        let rewritten = rewrite_chunk_with_fallbacks(
            &client,
            &api_key,
            chunk,
            &args.model,
            &args.fallback_models,
            args,
        )?;
        rewritten_chunks.push(rewritten);
    }

    Ok(rewritten_chunks.join("\n\n"))
}

fn rewrite_chunk_with_fallbacks(
    client: &Client,
    api_key: &str,
    chunk: &str,
    primary_model: &str,
    fallback_models: &[String],
    args: &Args,
) -> AppResult<String> {
    let mut models = Vec::new();
    models.push(primary_model.to_string());
    models.extend(fallback_models.iter().cloned());

    let mut last_error = String::new();

    for model in models {
        match rewrite_chunk_with_model(client, api_key, chunk, &model, args) {
            Ok(text) => return Ok(text),
            Err(e) => {
                last_error = e.to_string();
                eprintln!("Model '{}' failed: {}", model, last_error);
                if !fallback_models.is_empty() {
                    eprintln!("Trying next fallback model, if available...");
                }
            }
        }
    }

    Err(make_error(format!(
        "All OpenRouter models failed. Last error: {}",
        last_error
    )))
}

fn rewrite_chunk_with_model(
    client: &Client,
    api_key: &str,
    chunk: &str,
    model: &str,
    args: &Args,
) -> AppResult<String> {
    let system_prompt = build_system_prompt(args.preserve_markdown);
    let user_prompt = format!(
        "Rewrite the following text in a natural, professional human style.\n\n\
         Requirements:\n\
         - Preserve the meaning and technical accuracy.\n\
         - Preserve markdown headings, bold terms, examples, and paragraph breaks where possible.\n\
         - Avoid generic AI-sounding phrases.\n\
         - Improve sentence flow and clarity.\n\
         - Do not add unsupported claims.\n\
         - Return only the rewritten text.\n\n\
         Text:\n{}",
        chunk
    );

    let site_url = env::var("OPENROUTER_SITE_URL")
        .unwrap_or_else(|_| "https://github.com/humanize-text-rust".to_string());
    let app_title = env::var("OPENROUTER_APP_TITLE")
        .unwrap_or_else(|_| "Humanize Text Rust".to_string());

    let body = json!({
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": system_prompt
            },
            {
                "role": "user",
                "content": user_prompt
            }
        ],
        "temperature": args.temperature,
        "max_tokens": args.max_tokens,
        "reasoning": {
            "effort": "none",
            "exclude": true
        }
    });

    let mut attempt = 0;

    loop {
        attempt += 1;

        let response = client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .bearer_auth(api_key)
            .header("Content-Type", "application/json")
            .header("HTTP-Referer", &site_url)
            .header("X-Title", &app_title)
            .header("X-OpenRouter-Title", &app_title)
            .json(&body)
            .send()?;

        let status = response.status();
        let retry_after_header = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        let response_text = response.text()?;

        if status.is_success() {
            return parse_openrouter_text(&response_text);
        }

        if status.as_u16() == 429 && attempt <= args.max_retries {
            let retry_after = retry_after_header
                .or_else(|| parse_retry_after_from_error_json(&response_text))
                .unwrap_or(30);

            eprintln!(
                "OpenRouter rate limit reached for model '{}'. Retrying in {} seconds ({}/{})...",
                model, retry_after, attempt, args.max_retries
            );
            sleep(Duration::from_secs(retry_after));
            continue;
        }

        if status.as_u16() == 402 {
            return Err(make_error(format!(
                "OpenRouter returned 402 Payment Required / insufficient credits. \
                 Your API key is valid, but the account or organization needs credits, \
                 a different key, or a free model with available quota. Response: {}",
                response_text
            )));
        }

        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(make_error(format!(
                "OpenRouter authentication/permission failed with status {}. \
                 Check OPENROUTER_API_KEY and model access. Response: {}",
                status, response_text
            )));
        }

        return Err(make_error(format!(
            "OpenRouter request failed with status {}. Response: {}",
            status, response_text
        )));
    }
}

fn parse_openrouter_text(response_text: &str) -> AppResult<String> {
    let json_value: Value = serde_json::from_str(response_text).map_err(|e| {
        make_error(format!(
            "OpenRouter returned invalid JSON: {}. Raw response: {}",
            e, response_text
        ))
    })?;

    let choice = json_value
        .get("choices")
        .and_then(|choices| choices.get(0))
        .ok_or_else(|| make_error(format!("OpenRouter response had no choices: {}", response_text)))?;

    let finish_reason = choice
        .get("finish_reason")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");

    let message = choice.get("message").ok_or_else(|| {
        make_error(format!(
            "OpenRouter response choice had no message object: {}",
            response_text
        ))
    })?;

    let content_value = message.get("content").unwrap_or(&Value::Null);
    let mut content = extract_text_content(content_value).trim().to_string();

    // Some models return content inside reasoning by mistake if the request runs out of tokens.
    // We do not use reasoning as final output, but we report it so the user knows what happened.
    let reasoning_tokens = json_value
        .pointer("/usage/completion_tokens_details/reasoning_tokens")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);

    if content.is_empty() {
        return Err(make_error(format!(
            "OpenRouter returned no final rewritten text. finish_reason='{}', reasoning_tokens={}. \
             Increase --max-tokens, reduce --max-chunk-chars, or use a non-reasoning/editing model such as openai/gpt-4o-mini or openai/gpt-4.1-mini. \
             Raw response: {}",
            finish_reason, reasoning_tokens, response_text
        )));
    }

    if finish_reason == "length" {
        eprintln!(
            "Warning: model stopped because it hit the output token limit. The rewritten text may be incomplete. Try increasing --max-tokens."
        );
    }

    content = clean_model_output(&content);
    Ok(content)
}

fn extract_text_content(content: &Value) -> String {
    match content {
        Value::String(s) => s.to_string(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    Some(text.to_string())
                } else if let Some(text) = item.as_str() {
                    Some(text.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<String>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn clean_model_output(text: &str) -> String {
    let mut result = text.trim().to_string();

    // Remove accidental markdown fences if a model wraps the result.
    if result.starts_with("```") {
        let lines: Vec<&str> = result.lines().collect();
        if lines.len() >= 2 {
            let mut start = 1;
            let mut end = lines.len();
            if lines.last().map(|line| line.trim()) == Some("```") {
                end -= 1;
            }
            if lines[0].trim().starts_with("```") {
                start = 1;
            }
            result = lines[start..end].join("\n").trim().to_string();
        }
    }

    // Remove common labels that some models add despite instructions.
    let labels = [
        "Rewritten text:",
        "Rewritten Text:",
        "Here is the rewritten text:",
        "Here’s the rewritten text:",
    ];

    for label in labels {
        if result.starts_with(label) {
            result = result[label.len()..].trim().to_string();
        }
    }

    result
}

fn parse_retry_after_from_error_json(response_text: &str) -> Option<u64> {
    let value: Value = serde_json::from_str(response_text).ok()?;
    value
        .pointer("/error/metadata/retry_after_seconds")
        .and_then(|v| v.as_u64())
        .or_else(|| {
            value
                .pointer("/error/metadata/retry_after_seconds_raw")
                .and_then(|v| v.as_f64())
                .map(|v| v.ceil() as u64)
        })
}

fn build_system_prompt(preserve_markdown: bool) -> String {
    let markdown_instruction = if preserve_markdown {
        "Preserve markdown headings, lists, bold terms, and paragraph structure when useful."
    } else {
        "You may rewrite structure if it improves readability."
    };

    format!(
        "You are a professional technical editor. Rewrite text into a natural, clear, human professional style. \
         Keep the meaning, technical accuracy, examples, and scope intact. {} \
         Avoid filler, hype, generic AI phrasing, and unsupported claims. Return only the rewritten text.",
        markdown_instruction
    )
}

fn split_into_chunks(text: &str, max_chars: usize) -> Vec<String> {
    if text.chars().count() <= max_chars {
        return vec![text.trim().to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    for paragraph in text.split("\n\n") {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }

        if paragraph.chars().count() > max_chars {
            if !current.trim().is_empty() {
                chunks.push(current.trim().to_string());
                current.clear();
            }
            chunks.extend(split_long_paragraph(paragraph, max_chars));
            continue;
        }

        let proposed_len = current.chars().count() + paragraph.chars().count() + 2;
        if proposed_len > max_chars && !current.trim().is_empty() {
            chunks.push(current.trim().to_string());
            current.clear();
        }

        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(paragraph);
    }

    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }

    chunks
}

fn split_long_paragraph(paragraph: &str, max_chars: usize) -> Vec<String> {
    let sentences = split_sentences_keep_punctuation(paragraph);
    let mut chunks = Vec::new();
    let mut current = String::new();

    for sentence in sentences {
        let proposed_len = current.chars().count() + sentence.chars().count() + 1;
        if proposed_len > max_chars && !current.trim().is_empty() {
            chunks.push(current.trim().to_string());
            current.clear();
        }

        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(sentence.trim());
    }

    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }

    chunks
}

fn split_sentences_keep_punctuation(text: &str) -> Vec<String> {
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

fn apply_light_rewrites(text: &str) -> String {
    let mut result = text.to_string();

    let replacements = [
        ("It is important to note that ", ""),
        ("It should be noted that ", ""),
        ("It is worth noting that ", ""),
        ("In today's rapidly evolving digital landscape, ", ""),
        ("In today’s rapidly evolving digital landscape, ", ""),
        ("In the modern era, ", ""),
        ("This article aims to", "This article explains"),
        ("This paper aims to", "This paper explains"),
        ("plays a crucial role in", "helps"),
        ("plays a vital role in", "helps"),
        ("a wide range of", "many"),
        ("various different", "various"),
        ("in order to", "to"),
        ("due to the fact that", "because"),
        ("at this point in time", "now"),
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
        ("approximately", "about"),
        ("Approximately", "About"),
        ("prior to", "before"),
        ("Prior to", "Before"),
        ("subsequent to", "after"),
        ("Subsequent to", "After"),
    ];

    for (from, to) in replacements {
        result = result.replace(from, to);
    }

    result = apply_generic_regex_rewrites(&result);
    result = rewrite_numbered_sections(&result);
    clean_spacing_preserve_paragraphs(&result)
}

fn apply_generic_regex_rewrites(text: &str) -> String {
    let mut result = text.to_string();

    let regex_rewrites = [
        (
            r"This is where \*\*([^*]+)\*\* becomes useful\.",
            "This is where **$1** helps.",
        ),
        (
            r"This is where ([A-Za-z0-9 ,\-]+) becomes useful\.",
            "This is where $1 helps.",
        ),
        (
            r"In simple terms, ([^.]+?) helps teams move from",
            "Simply put, $1 moves teams from",
        ),
        (
            r"([A-Za-z ]+) is no longer a simple path from ([^.]+?) to ([^.]+?) and then to ([^.]+?)\.",
            "$1 no longer moves in a straight line from $2 to $3 to $4.",
        ),
        (
            r"Instead of treating ([^.]+?) as a one-time activity, it treats ([^.]+?) as ([^.]+?)\.",
            "Rather than treating $1 as a one-time activity, it treats $2 as $3.",
        ),
    ];

    for (pattern, replacement) in regex_rewrites {
        let re = Regex::new(pattern).expect("valid regex");
        result = re.replace_all(&result, replacement).to_string();
    }

    result
}

fn rewrite_numbered_sections(text: &str) -> String {
    let mut result = text.to_string();

    let patterns = [
        (r"The first is the \*\*([^*]+)\*\*\.", "First comes the **$1**."),
        (r"The second is the \*\*([^*]+)\*\*\.", "Next is the **$1**."),
        (r"The third is the \*\*([^*]+)\*\*\.", "The third area is **$1**."),
        (r"The fourth is the \*\*([^*]+)\*\*\.", "The fourth area is **$1**."),
        (r"The fifth is the \*\*([^*]+)\*\*\.", "The fifth area is **$1**."),
        (r"The sixth is the \*\*([^*]+)\*\*\.", "The final area is **$1**."),
        (r"The first is ([^.]+)\.", "First comes $1."),
        (r"The second is ([^.]+)\.", "Next is $1."),
        (r"The third is ([^.]+)\.", "The third area is $1."),
        (r"The fourth is ([^.]+)\.", "The fourth area is $1."),
        (r"The fifth is ([^.]+)\.", "The fifth area is $1."),
        (r"The sixth is ([^.]+)\.", "The final area is $1."),
    ];

    for (pattern, replacement) in patterns {
        let re = Regex::new(pattern).expect("valid regex");
        result = re.replace_all(&result, replacement).to_string();
    }

    result
}

fn clean_spacing_preserve_paragraphs(text: &str) -> String {
    let punctuation_space = Regex::new(r"\s+([,.!?;:])").expect("valid regex");
    let multiple_spaces = Regex::new(r"[ ]{2,}").expect("valid regex");

    text.lines()
        .map(|line| {
            let line = punctuation_space.replace_all(line, "$1").to_string();
            multiple_spaces.replace_all(&line, " ").trim().to_string()
        })
        .collect::<Vec<String>>()
        .join("\n")
}

fn make_error(message: impl Into<String>) -> Box<dyn std::error::Error> {
    Box::new(io::Error::new(io::ErrorKind::Other, message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_rewrite_changes_generic_patterns() {
        let input = "The first is the **intent loop**. This is where **Loop Engineering** becomes useful.";
        let output = apply_light_rewrites(input);
        assert!(output.contains("First comes the **intent loop**."));
        assert!(output.contains("This is where **Loop Engineering** helps."));
    }

    #[test]
    fn split_chunks_preserves_short_text() {
        let chunks = split_into_chunks("One paragraph.", 1000);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn extracts_string_content() {
        let value = Value::String("hello".to_string());
        assert_eq!(extract_text_content(&value), "hello");
    }
}
