use std::fs;
use std::io;

/// Load a skill prompt from disk and combine it with the source article.
/// This can be used before sending the request to OpenRouter.
pub fn build_prompt_from_skill(skill_path: &str, input_text: &str) -> io::Result<String> {
    let skill = fs::read_to_string(skill_path)?;

    let prompt = format!(
        "{skill}\n\n---\n\nSOURCE TEXT TO REWRITE:\n\n{input}\n\n---\n\nRewrite the source text using the skill instructions above. Return only the final article.",
        skill = skill,
        input = input_text
    );

    Ok(prompt)
}

fn main() -> io::Result<()> {
    let skill_path = "skills/loop_engineering_reference_article/SKILL.md";
    let input_path = "examples/loop_engineering_input.txt";

    let input_text = fs::read_to_string(input_path)?;
    let prompt = build_prompt_from_skill(skill_path, &input_text)?;

    println!("{}", prompt);
    Ok(())
}
