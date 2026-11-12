---
name: update-rust-code-comments
description: "Review, clarify, or update Rust implementation comments. Use for comment cleanup, design-intent comments, or changes to source-comment conventions."
argument-hint: "Rust implementation file(s) or modules to update"
---

# Update Rust Code Comments

## When to Use

- The user asks to review, clarify, refactor, or standardize comments in Rust implementation code.
- A code review identifies unclear intent, undocumented constraints, or redundant comments.
- The project changes its source-comment convention.

## Scope

This skill covers Rust implementation source, not tests. For Rust test code, including test modules and test helpers, use `update-rust-test-comments`.

## Comment Principles

- Base every claim on analysis of the relevant code. Tests, assertions, call sites, and project documentation may corroborate that analysis but do not replace it. Do not infer behavior, hardware semantics, compatibility, or design intent from names alone. If the code and corroborating evidence are insufficient, omit the claim or ask for clarification.
- Follow repository-specific comment requirements before applying this skill's general advice. In this repository, explain the module design in human-readable terms and comment significant logical groups where the code supports an explanation.
- Explain intent, invariants, constraints, or trade-offs that are not clear from the code itself.
- Keep comments accurate and close to the code they explain; update or remove them when the implementation changes.
- For public APIs, document caller-relevant behavior, guarantees, constraints, or invariants rather than repeating the signature.
- Avoid narrating visible operations, restating names or values, and duplicating purpose comments at internal blocks.
- Do not add comments to trivial or self-explanatory statements. The requirement to explain significant logical groups does not mean commenting every line.

## Workflow

1. Follow the requested file and module scope; do not expand it unless asked.
2. Read the selected implementation and enough related code, tests, and call sites to establish its behavior and design intent. Do not write a claim until its source is understood.
3. Identify comments that are missing, inaccurate, redundant, misplaced, or inconsistent with the project's conventions. Apply any project-specific comment requirement; do not expand it into comments on every statement.
4. Make precise comment edits. Preserve executable behavior and change code structure only when requested.
5. Review the edited scope claim by claim against the inspected evidence. Check that the module overview and significant group comments explain verified design rather than paraphrasing code; report unclear areas instead of guessing.
6. Run the relevant Rust formatter when available. For Cargo projects, use the repository's existing workspace or package scope. Run focused tests or documentation checks when examples or generated documentation could be affected, or when requested.
7. Follow the project's review workflow when changes go beyond comments.

## Completion

Report the files and comment intent changed, validation results, and any out-of-scope files that may still need attention.
