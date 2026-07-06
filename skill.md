# Loop Engineering Reference Article Skill

## Purpose
Rewrite AI-assisted software engineering text into a publication-style practitioner article about Loop Engineering.

This skill is designed for Rust CLI programs that call an LLM through OpenRouter or another chat-completions API. The Rust program should load this file, add the user's source article, and ask the model to produce the final article.

## When to Use
Use this skill when the input text is about:

- Loop Engineering
- AI-assisted development
- AI-generated code, tests, documentation, APIs, or UI components
- software delivery feedback loops
- engineering validation, review, runtime feedback, and business outcomes
- modernization examples such as Ant to Maven/Gradle, monolith to microservices, MQ to Kafka, or manual deployment to CI/CD

Do not use this skill for general summarization, generic grammar correction, or detector bypassing.

## Target Style
Write like a senior software engineering practitioner explaining lessons to other developers and architects.

The style should be:

- clear
- practical
- technically accurate
- publication-ready
- not academic
- not marketing-heavy
- not overly casual
- not artificially imperfect

Use direct engineering language. Prefer concrete examples over broad claims.

## Required Article Structure
Use this structure unless the user explicitly asks for something else:

```text
# Loop Engineering: Building Better Software Through Feedback

## Key Takeaways
## Why This Topic Matters
## What Is Loop Engineering?
## AI-Assisted Development as the Constraint
## What Hurts
## What Actually Helps
## The Six Feedback Loops
## Example: AI-Assisted UI Generation
## Example: Modernization
## Bringing It All Together
## Conclusion
```

## Section Guidance

### Key Takeaways
Create 4-6 concise bullet points. Each bullet should be specific and useful.

Avoid generic phrases such as:

- "game changer"
- "rapidly evolving landscape"
- "robust framework"
- "criticality"
- "seamless"
- "transformative"

Good takeaway examples:

- AI-generated code still needs validation, review, and runtime feedback before teams can trust it.
- Loop Engineering helps teams move from generating artifacts to proving that those artifacts are safe and useful.
- Feedback loops should cover intent, generation, validation, human review, runtime behavior, and business outcomes.

### Why This Topic Matters
Explain why AI-assisted development changes software delivery.

Include these ideas:

- AI increases the speed and volume of generated work.
- Speed alone does not make software production-ready.
- Generated code may compile but miss business rules.
- Generated tests may pass while checking the wrong scenario.
- Teams need evidence before trusting generated work.

### What Is Loop Engineering?
Define Loop Engineering clearly.

Use this idea:

Loop Engineering is the practice of placing feedback loops around every important engineering output so teams can generate, validate, review, learn, and improve continuously.

Mention that it connects existing practices such as:

- Agile
- DevOps
- CI/CD
- observability
- architecture governance
- DORA metrics

### AI-Assisted Development as the Constraint
Explain that the bottleneck changes.

Before AI, teams often focused on writing code faster. With AI, the harder problem becomes validating, reviewing, securing, and integrating generated output.

### What Hurts
Focus on AI-specific delivery problems.

Include practical problems such as:

- vague prompts
- large generated changes that are hard to review
- tests that pass but do not validate the right behavior
- generated APIs that ignore logging, exception handling, security, or deployment standards
- generated UI that misses accessibility, design, or API contract requirements
- runtime issues that are not fed back into future prompts or requirements

### What Actually Helps
Explain practical controls.

Include:

- small, bounded AI-generated changes
- clear intent before generation
- automated validation in CI/CD
- architecture rules as code
- security and dependency scanning
- human review by engineers, architects, product owners, and security reviewers
- runtime observability
- evidence collection for review and audit

### The Six Feedback Loops
Explain the six loops in practical language:

1. Intent loop
2. Generation loop
3. Validation loop
4. Human review loop
5. Runtime feedback loop
6. Business outcome loop

Do not make this section sound like a textbook. Use short explanations and practical examples.

### Example: AI-Assisted UI Generation
Use a simple scenario:

A product owner describes a dashboard in plain English. AI generates a React component. Loop Engineering checks the component against design standards, accessibility rules, API contracts, security expectations, tests, and user feedback.

End with the idea that the result is not just a generated screen; it becomes a reviewed and validated product feature.

### Example: Modernization
Use enterprise modernization examples:

- Ant to Maven or Gradle
- monolith to microservices
- MQ to Kafka
- manual deployment to CI/CD

Explain how Loop Engineering validates each step through builds, tests, reviews, deployment evidence, and runtime feedback.

### Bringing It All Together
Explain that Loop Engineering adds discipline to AI-assisted delivery.

Generated work should prove itself through validation, review, and feedback before teams rely on it.

### Conclusion
End with the principle:

Loop Engineering does not replace Agile, DevOps, CI/CD, or architecture governance. It connects them. Every engineering output should create feedback, and every feedback signal should improve the next engineering decision.

## Tone Rules

Prefer:

- "AI-generated code may compile and still miss a business rule."
- "The validation loop is where generated work has to prove itself."
- "The goal is not to generate more code. The goal is to generate work the team can trust."

Avoid:

- "in today's rapidly evolving digital landscape"
- "criticality"
- "robust mechanisms"
- "transformative potential"
- "seamless integration"
- "meticulously"
- "invaluable"
- "cohesive, self-improving system"
- "advent of AI"
- "business objectives" when "business goals" or "business problem" would be clearer

## Output Rules

- Return only the rewritten article.
- Do not include commentary about the rewrite.
- Preserve technical meaning.
- Do not invent unsupported claims.
- Do not claim real-world project experience unless it is present in the source text.
- Keep markdown headings.
- Keep the article suitable for DZone, BizTechFoundation, or similar practitioner publications.
- Avoid adding fake statistics, fake references, or fake citations.

## Final Quality Check
Before returning the article, check for and fix:

- awkward grammar
- repeated transitions
- overly polished corporate wording
- too many abstract claims
- unsupported claims
- AI-like phrases
- casual phrases that sound unprofessional
- missing commas in long sentences
- incorrect wording such as "strongness," "reviewing importantly," "a important," or "fit with out architecture"
