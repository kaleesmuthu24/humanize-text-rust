# Humanize Text Rust

A simple Rust utility that converts AI-generated or overly formal text into a more natural, readable format.

This project uses **Loop Engineering** as the sample article.

## What this tool does

- Preserves markdown headings
- Keeps paragraph breaks
- Preserves bold markdown terms such as `**intent loop**`
- Simplifies overly formal words
- Removes common AI-style phrases
- Splits very long sentences
- Keeps a professional article tone

This tool does **not** call any AI API. It is a rule-based Rust program.

## Run in GitHub Codespaces

### 1. Open the project in Codespaces

Upload this project to GitHub, open the repository, and choose:

```text
Code -> Codespaces -> Create codespace on main
```

The included `.devcontainer/devcontainer.json` uses a Rust development container.

### 2. Build the project

```bash
cargo build
```

### 3. Run the Loop Engineering example

```bash
cargo run -- examples/loop_engineering.txt loop_engineering_human.txt
```

### 4. View the output

```bash
cat loop_engineering_human.txt
```

## Use your own article

Create a file:

```bash
nano examples/my_article.txt
```

Paste your AI-generated text or article content into it.

Run:

```bash
cargo run -- examples/my_article.txt my_article_human.txt
```

View:

```bash
cat my_article_human.txt
```

## Run tests

```bash
cargo test
```

## Notes

This is a practical utility for cleaning and improving article-style text. It cannot guarantee perfect human writing, but it helps reduce repetitive AI-style wording and improves readability while preserving the original structure.
# humanize-text-rust
