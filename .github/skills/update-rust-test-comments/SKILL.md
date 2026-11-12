---
name: update-rust-test-comments
description: "Review, clarify, or update comments in Rust tests. Use for test rationale, setup and phase comments, or changes to test-comment conventions."
argument-hint: "Rust test file(s) or modules to update"
---

# Update Rust Test Comments

## When to Use

- The user asks to review, refactor, or standardize comments in Rust tests.
- A test review identifies unclear rationale, setup, phase boundaries, or state transitions.
- The project changes its test-comment convention.

## Scope

This skill covers Rust tests, including test modules and test helpers. Follow the project test-location rule: keep test bodies and helpers in separate files, not implementation files. For Rust implementation source, use `update-rust-code-comments`.

## Comment Principles

- Base every comment on the test and the behavior it exercises. Read the relevant implementation and, where needed, callers or project documentation to verify what the test demonstrates and why it matters. Do not invent expected behavior, rationale, or failure consequences; if evidence is insufficient, ask or report the gap.
- Follow repository-specific test-comment requirements before applying this skill's general advice. In this repository, every test starts with brief **What** and **Why** comments and includes meaningful **Arrange**, **Act**, and **Assert** comments.
- Put the phase comments at the actual boundaries. Explain the test's real initial conditions or fixture, operation, and observable assertion/result. If there is no separate setup statement, describe the verified initial condition rather than fabricating a setup step. For multi-stage tests, explain distinct transitions when they affect the behavior being verified.
- Make the comments useful to a human reader, not just phase labels or paraphrases of statements. Keep them concise, and avoid repeating the same rationale or expectation.

## Workflow

1. Follow the requested test scope; do not expand to the full test suite unless asked.
2. Read the selected tests and enough related implementation, call sites, and documentation to establish the behavior, setup, constraints, and observable assertions before drafting comments.
3. Identify comments that are missing, inaccurate, redundant, misplaced, or inconsistent with project conventions.
4. Make precise comment edits. Preserve executable behavior and meaningful assertions; avoid unrelated test or production-code cleanup.
5. Review every test against the required convention: What/Why at the start and evidence-based Arrange/Act/Assert explanations. Verify each claim against the test and relevant implementation; do not treat the presence of headings as proof of comment quality. Report uncertainty rather than guessing.
6. Run the relevant Rust formatter when available. For Cargo projects, use the repository's existing workspace or package scope. Run focused tests when test behavior or code beyond comments changes, examples or doctests may be affected, or when requested.
7. Follow the project's review workflow when changes go beyond comments.

## Completion

Report the files and comment intent changed, validation results, and any out-of-scope tests that may still need attention.
