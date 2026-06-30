use std::env;
use std::fs;
use std::process;

/// Humanize AI-generated or overly formal text while preserving article structure.
///
/// What it does:
/// - Preserves headings and markdown formatting
/// - Keeps paragraph breaks
/// - Simplifies overly formal wording
/// - Removes common AI-style phrases
/// - Splits very long sentences
/// - Keeps the tone professional
///
/// Usage:
/// cargo run -- examples/loop_engineering.txt output.txt
fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 3 {
        eprintln!("Usage: cargo run -- <input.txt> <output.txt>");
        process::exit(1);
    }

    let input_file = &args[1];
    let output_file = &args[2];

    let input_text = match fs::read_to_string(input_file) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("Failed to read input file '{}': {}", input_file, err);
            process::exit(1);
        }
    };

    let output = humanize_article(&input_text);

    if let Err(err) = fs::write(output_file, output) {
        eprintln!("Failed to write output file '{}': {}", output_file, err);
        process::exit(1);
    }

    println!("Completed successfully.");
    println!("Human-readable version saved to: {}", output_file);
}

fn humanize_article(text: &str) -> String {
    let mut output = Vec::new();

    for block in text.split("\n\n") {
        let trimmed = block.trim();

        if trimmed.is_empty() {
            continue;
        }

        // Preserve markdown headings without rewriting them heavily.
        if trimmed.starts_with('#') {
            output.push(trimmed.to_string());
            continue;
        }

        let mut paragraph = normalize_spaces(trimmed);
        paragraph = remove_ai_style_phrases(&paragraph);
        paragraph = simplify_formal_words(&paragraph);
        paragraph = improve_article_flow(&paragraph);
        paragraph = split_long_sentences(&paragraph);

        output.push(paragraph.trim().to_string());
    }

    output.join("\n\n")
}

fn normalize_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<&str>>().join(" ")
}

fn remove_ai_style_phrases(text: &str) -> String {
    let replacements = vec![
        ("In today's rapidly evolving digital landscape,", ""),
        ("In today’s rapidly evolving digital landscape,", ""),
        ("In the modern era,", ""),
        ("It is important to note that", ""),
        ("It should be noted that", ""),
        ("This article aims to", "This article explains"),
        ("This paper aims to", "This paper explains"),
        ("plays a crucial role in", "helps with"),
        ("plays a vital role in", "helps with"),
        ("a wide range of", "many"),
        ("various different", "various"),
        ("in order to", "to"),
        ("due to the fact that", "because"),
        ("at this point in time", "now"),
        ("firstly", "first"),
        ("Secondly", "Second"),
        ("secondly", "second"),
        ("Lastly", "Finally"),
        ("lastly", "finally"),
    ];

    apply_replacements(text, replacements)
}

fn simplify_formal_words(text: &str) -> String {
    let replacements = vec![
        ("utilize", "use"),
        ("Utilize", "Use"),
        ("leverage", "use"),
        ("Leverage", "Use"),
        ("facilitates", "helps"),
        ("Facilitates", "Helps"),
        ("approximately", "about"),
        ("Approximately", "About"),
        ("demonstrates", "shows"),
        ("Demonstrates", "Shows"),
        ("methodology", "method"),
        ("Methodology", "Method"),
        ("prior to", "before"),
        ("Prior to", "Before"),
        ("subsequent to", "after"),
        ("Subsequent to", "After"),
        ("numerous", "many"),
        ("Numerous", "Many"),
        ("commence", "start"),
        ("Commence", "Start"),
        ("terminate", "end"),
        ("Terminate", "End"),
        ("obtain", "get"),
        ("Obtain", "Get"),
        ("sufficient", "enough"),
        ("Sufficient", "Enough"),
        ("therefore", "so"),
        ("Therefore", "So"),
    ];

    apply_replacements(text, replacements)
}

fn improve_article_flow(text: &str) -> String {
    let replacements = vec![
        (
            "This is where **Loop Engineering** becomes useful.",
            "This is where **Loop Engineering** helps.",
        ),
        (
            "The biggest benefit of Loop Engineering is discipline.",
            "The biggest benefit of Loop Engineering is the discipline it brings to software delivery.",
        ),
        (
            "Software delivery is valuable only when it supports a real business outcome.",
            "Software delivery creates value only when it supports a real business outcome.",
        ),
        (
            "A faster release is helpful only if it improves customer experience, reliability, compliance, cost, or operational efficiency.",
            "A faster release matters only when it improves customer experience, reliability, compliance, cost, or operational efficiency.",
        ),
        (
            "The goal should not be to generate as much as possible.",
            "The goal is not to generate as much as possible.",
        ),
    ];

    apply_replacements(text, replacements)
}

fn apply_replacements(text: &str, replacements: Vec<(&str, &str)>) -> String {
    let mut result = text.to_string();

    for (from, to) in replacements {
        result = result.replace(from, to);
    }

    result
}

fn split_long_sentences(text: &str) -> String {
    let sentences = split_into_sentences(text);
    let mut output = Vec::new();

    for sentence in sentences {
        let word_count = sentence.split_whitespace().count();

        if word_count > 36 {
            output.push(split_sentence(&sentence));
        } else {
            output.push(sentence);
        }
    }

    output.join(" ")
}

fn split_into_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        current.push(ch);

        if ch == '.' || ch == '!' || ch == '?' {
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

fn split_sentence(sentence: &str) -> String {
    let split_markers = vec![
        " because ",
        " while ",
        " although ",
        " however ",
        " therefore ",
        " and ",
        " but ",
    ];

    for marker in split_markers {
        if let Some(index) = sentence.find(marker) {
            let first = sentence[..index].trim();
            let second = sentence[index + marker.len()..].trim();

            let first_count = first.split_whitespace().count();
            let second_count = second.split_whitespace().count();

            if first_count > 12 && second_count > 8 {
                return format!(
                    "{}. {}",
                    clean_sentence_end(first),
                    capitalize_first(second)
                );
            }
        }
    }

    sentence.to_string()
}

fn clean_sentence_end(text: &str) -> String {
    text.trim()
        .trim_end_matches(',')
        .trim_end_matches('.')
        .to_string()
}

fn capitalize_first(text: &str) -> String {
    let mut chars = text.chars();

    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_heading() {
        let input = "# Loop Engineering\n\nThis is where **Loop Engineering** becomes useful.";
        let output = humanize_article(input);
        assert!(output.contains("# Loop Engineering"));
        assert!(output.contains("This is where **Loop Engineering** helps."));
    }

    #[test]
    fn simplifies_formal_words() {
        let input = "Teams utilize feedback loops in order to improve delivery.";
        let output = humanize_article(input);
        assert!(output.contains("Teams use feedback loops to improve delivery."));
    }
}
