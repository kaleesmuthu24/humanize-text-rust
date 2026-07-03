use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, ValueEnum};
use regex::Regex;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

const OPENROUTER_CHAT_COMPLETIONS_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

#[derive(Parser, Debug)]
#[command(name = "humanize-text")]
#[command(version = "0.4.0")]
#[command(about = "Rewrite AI-generated text into clearer, natural professional writing.")]
struct Cli {
    /// Input text or markdown file
    input: PathBuf,

    /// Output file
    output: PathBuf,

    /// Rewrite mode: light is local/rule-based, strong uses OpenRouter
    #[arg(long, value_enum, default_value_t = Mode::Strong)]
    mode: Mode,

    /// OpenRouter model slug. Use openrouter/free when you do not have credits.
    #[arg(long, default_value = "openrouter/free")]
    model: String,

    /// OpenRouter API key. Prefer OPENROUTER_API_KEY or .env instead of this flag.
    #[arg(long, env = "OPENROUTER_API_KEY")]
    api_key: Option<String>,

    /// Optional site/app URL sent to OpenRouter for attribution
    #[arg(long, default_value = "https://github.com")]
    referer: String,

    /// Optional app title sent to OpenRouter for attribution
    #[arg(long, default_value = "Humanize Text Rust")]
    app_title: String,

    /// Creativity level for rewriting. Lower values preserve the original more closely.
    #[arg(long, default_value_t = 0.35)]
    temperature: f32,

    /// Approximate maximum characters per request chunk. Smaller chunks reduce token cost.
    #[arg(long, default_value_t = 6_000)]
    max_chunk_chars: usize,

    /// Maximum output tokens requested from OpenRouter. Lower values reduce cost.
    #[arg(long, default_value_t = 1200)]
    max_tokens: u32,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum Mode {
    Light,
    Strong,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize, Debug)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize, Debug)]
struct Choice {
    message: AssistantMessage,
}

#[derive(Deserialize, Debug)]
struct AssistantMessage {
    content: Option<String>,
}

fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    let input_text = fs::read_to_string(&cli.input)
        .with_context(|| format!("Failed to read input file: {}", cli.input.display()))?;

    let result = match cli.mode {
        Mode::Light => humanize_light(&input_text)?,
        Mode::Strong => {
            let api_key = get_api_key(cli.api_key)?;
            let client = build_openrouter_client(&api_key, &cli.referer, &cli.app_title)?;
            humanize_strong_with_openrouter(
                &client,
                &input_text,
                &cli.model,
                cli.temperature,
                cli.max_chunk_chars,
                cli.max_tokens,
            )?
        }
    };

    fs::write(&cli.output, result)
        .with_context(|| format!("Failed to write output file: {}", cli.output.display()))?;

    println!("Completed successfully.");
    println!("Output saved to: {}", cli.output.display());

    Ok(())
}

fn get_api_key(api_key_from_cli_or_env: Option<String>) -> Result<String> {
    if let Some(key) = api_key_from_cli_or_env {
        let trimmed = key.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }

    if let Ok(key) = env::var("OPENROUTER_API_KEY") {
        let trimmed = key.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }

    bail!(
        "Missing OpenRouter API key. Set OPENROUTER_API_KEY, create a .env file, or pass --api-key. Example: export OPENROUTER_API_KEY=\"sk-or-...\""
    )
}

fn build_openrouter_client(api_key: &str, referer: &str, app_title: &str) -> Result<Client> {
    let mut headers = HeaderMap::new();

    let auth_value = format!("Bearer {}", api_key);
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&auth_value).context("Invalid API key header value")?,
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    // OpenRouter recommends these optional attribution headers.
    headers.insert(
        "HTTP-Referer",
        HeaderValue::from_str(referer).context("Invalid HTTP-Referer header value")?,
    );
    headers.insert(
        "X-OpenRouter-Title",
        HeaderValue::from_str(app_title).context("Invalid X-OpenRouter-Title header value")?,
    );

    Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(120))
        .build()
        .context("Failed to build HTTP client")
}

fn humanize_strong_with_openrouter(
    client: &Client,
    input_text: &str,
    model: &str,
    temperature: f32,
    max_chunk_chars: usize,
    max_tokens: u32,
) -> Result<String> {
    let chunks = split_markdown_into_chunks(input_text, max_chunk_chars.max(2_000));
    let mut rewritten_chunks = Vec::new();

    for (index, chunk) in chunks.iter().enumerate() {
        println!("Rewriting chunk {}/{} with OpenRouter...", index + 1, chunks.len());
        let rewritten = rewrite_chunk(client, chunk, model, temperature, max_tokens)?;
        rewritten_chunks.push(rewritten.trim().to_string());
    }

    Ok(rewritten_chunks.join("\n\n"))
}

fn rewrite_chunk(client: &Client, chunk: &str, model: &str, temperature: f32, max_tokens: u32) -> Result<String> {
    let system_prompt = r#"You are a senior technical editor.
Rewrite the user's text into natural, human, professional writing.
Preserve the original meaning, technical accuracy, markdown headings, bullet lists, bold text, and code blocks.
Make the output visibly improved: reduce AI-sounding phrasing, vary sentence structure, remove repetition, and improve flow.
Do not add unsupported claims, fake citations, fake metrics, or new achievements.
Do not explain your changes. Return only the rewritten text."#;

    let user_prompt = format!(
        "Rewrite the following article section in a natural professional style while preserving meaning and markdown:\n\n{}",
        chunk
    );

    let request = ChatRequest {
        model: model.to_string(),
        temperature,
        max_tokens,
        messages: vec![
            Message {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            Message {
                role: "user".to_string(),
                content: user_prompt,
            },
        ],
    };

    let response = client
        .post(OPENROUTER_CHAT_COMPLETIONS_URL)
        .json(&request)
        .send()
        .context("Failed to send request to OpenRouter")?;

    let status = response.status();
    let body = response
        .text()
        .context("Failed to read OpenRouter response body")?;

    if !status.is_success() {
        if status.as_u16() == 401 || status.as_u16() == 403 {
            bail!(
                "OpenRouter authentication failed with status {}. Check OPENROUTER_API_KEY and account access. Response: {}",
                status,
                body
            );
        }

        if status.as_u16() == 402 {
            bail!(
                "OpenRouter returned 402 Payment Required. This means the selected model needs credits, the API key has a zero/negative balance, or the account/key credit limit blocks the request. Fix options: add credits in OpenRouter, choose a free model with --model openrouter/free, reduce cost with --max-tokens 800, or check that OPENROUTER_API_KEY belongs to the correct account. Response: {}",
                body
            );
        }

        if status.as_u16() == 429 {
            bail!(
                "OpenRouter rate limit reached with status {}. Try again later, choose another model, or reduce chunk size with --max-chunk-chars. Response: {}",
                status,
                body
            );
        }

        bail!("OpenRouter request failed with status {}. Response: {}", status, body);
    }

    let parsed: ChatResponse = serde_json::from_str(&body)
        .with_context(|| format!("Failed to parse OpenRouter JSON response: {}", body))?;

    parsed
        .choices
        .first()
        .and_then(|choice| choice.message.content.as_ref())
        .map(|content| content.trim().to_string())
        .ok_or_else(|| anyhow!("OpenRouter response did not contain assistant content: {}", body))
}

fn split_markdown_into_chunks(text: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut in_code_block = false;

    for paragraph in text.split("\n\n") {
        let trimmed = paragraph.trim_end();

        if trimmed.lines().any(|line| line.trim_start().starts_with("```")) {
            let fence_count = trimmed
                .lines()
                .filter(|line| line.trim_start().starts_with("```"))
                .count();
            if fence_count % 2 == 1 {
                in_code_block = !in_code_block;
            }
        }

        let candidate_len = current.len() + trimmed.len() + 2;

        if !current.is_empty() && candidate_len > max_chars && !in_code_block {
            chunks.push(current.trim().to_string());
            current.clear();
        }

        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(trimmed);
    }

    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }

    chunks
}

fn humanize_light(input: &str) -> Result<String> {
    let mut blocks = Vec::new();
    let code_fence = Regex::new(r"(?m)^```").unwrap();
    let mut in_code = false;

    for block in input.split("\n\n") {
        let trimmed = block.trim();
        if trimmed.is_empty() {
            continue;
        }

        let fence_count = code_fence.find_iter(trimmed).count();
        let preserve = in_code || trimmed.starts_with('#') || trimmed.starts_with('|') || trimmed.starts_with("```");

        let processed = if preserve {
            trimmed.to_string()
        } else {
            let normalized = normalize_spaces(trimmed);
            let replaced = apply_light_rewrites(&normalized);
            split_overlong_sentences(&replaced)
        };

        if fence_count % 2 == 1 {
            in_code = !in_code;
        }

        blocks.push(processed);
    }

    Ok(blocks.join("\n\n"))
}

fn normalize_spaces(text: &str) -> String {
    let re = Regex::new(r"\s+").unwrap();
    re.replace_all(text, " ").trim().to_string()
}

fn apply_light_rewrites(text: &str) -> String {
    let replacements = [
        ("Software development is no longer a simple path from requirement to code and then to release.", "Software development no longer moves in a straight line from requirements to code to release."),
        ("This is where **Loop Engineering** becomes useful.", "This is where **Loop Engineering** helps."),
        ("In simple terms, Loop Engineering helps teams move from “generate and hope” to “generate, validate, learn, and improve.”", "Simply put, Loop Engineering moves teams from “generate and hope” to “generate, validate, learn, and improve.”"),
        ("The first is the **intent loop**.", "First comes the **intent loop**."),
        ("The second is the **generation loop**.", "Next is the **generation loop**."),
        ("The third is the **validation loop**.", "The third loop is **validation**."),
        ("The fourth is the **human review loop**.", "The fourth loop is **human review**."),
        ("The fifth is the **runtime feedback loop**.", "The fifth loop is **runtime feedback**."),
        ("The sixth is the **business outcome loop**.", "The final loop is the **business outcome loop**."),
        ("It is important to note that", ""),
        ("It should be noted that", ""),
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

    let mut result = text.to_string();
    for (from, to) in replacements {
        result = result.replace(from, to);
    }
    normalize_spaces(&result)
}

fn split_overlong_sentences(text: &str) -> String {
    let mut output = Vec::new();

    for sentence in split_sentences(text) {
        let word_count = sentence.split_whitespace().count();
        if word_count <= 34 {
            output.push(sentence);
            continue;
        }

        let markers = [" because ", " while ", " but ", " and ", " which "];
        let mut changed = false;

        for marker in markers {
            if let Some(index) = sentence.find(marker) {
                let first = sentence[..index].trim().trim_end_matches(',').trim_end_matches('.');
                let second = sentence[index + marker.len()..].trim();
                if first.split_whitespace().count() > 10 && second.split_whitespace().count() > 8 {
                    output.push(format!("{}.", first));
                    output.push(capitalize_first(second));
                    changed = true;
                    break;
                }
            }
        }

        if !changed {
            output.push(sentence);
        }
    }

    output.join(" ")
}

fn split_sentences(text: &str) -> Vec<String> {
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

fn capitalize_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
