# Project Guidelines

## Working in This Fork

- Treat files inherited from upstream as read-only. Modify them only when the user explicitly authorizes the particular change; otherwise, prefer new Rust-port files or project-local Copilot configuration.
- Preserve existing emulator behavior and supplied program/data assets unless a requested change requires otherwise.
- Keep changes focused and do not overwrite unrelated local work.

## Handoff and Daily Chat History

- At the start of each chat, read `dev/copilot/chats/tek4051/tek4051-handoff.md` and the current local date's history at `dev/copilot/chats/tek4051/session-history/session-history-YYYYMMDD.md`, if present.
- Use one history file per local calendar date, with digits-only `YYYYMMDD` in its filename. Create today's file when recording the first chat of that day.
- Append a concise, factual entry for each chat to that day's file. Record requested work, changes, decisions, validation results, blockers, and next actions where relevant; preserve earlier entries rather than replacing them.
- At the end of each chat, update the handoff with the current project status, decisions, blockers, and next useful action. Keep it a current summary rather than a duplicate of the history.
- Do not record secrets or unrelated personal information in either document.

## Rust and egui

- Use idiomatic Rust, clear ownership, and explicit error handling. Surface errors rather than silently substituting defaults or hiding failures.
- Follow established egui patterns. Keep emulator behavior testable independently of UI code where practical, without introducing unnecessary architecture or abstractions.

### Rust comments

- Ground every comment and documentation claim in analysis of the relevant code. Tests, assertions, call sites, and project documentation may corroborate that analysis but do not replace it. Do not infer behavior or design intent from names alone. If the code and corroborating evidence do not establish a claim, omit it or ask for clarification rather than inventing content.
- In implementation files, provide a human-readable module overview and explain significant logical groups where the code supports it. Describe verified responsibilities, control/data flow, invariants, API behavior, error handling, and compatibility constraints; do not narrate obvious statements or claim unsupported rationale.
- In tests, every test must start with brief **What** and **Why** comments and include meaningful **Arrange**, **Act**, and **Assert** comments. Explain the actual setup or preconditions, operation, and observable assertion/result. For a test with no separate setup, describe its actual initial condition instead of fabricating an Arrange step. Comments must explain intent, not merely supply labels or repeat code.
- Keep comments accurate, close to the code they explain, and concise without sacrificing the requested human-readable explanation. These repository-specific requirements take precedence over generic guidance to comment only when a comment seems optional; they do not require comments on every statement.

## Implementation and Validation

- Work in small, buildable increments and follow the existing project structure and conventions.
- Add focused tests for observable behavior, important boundaries, and error paths. Prefer meaningful assertions over implementation-detail checks, and do not widen production visibility solely to facilitate tests.
- Run the existing formatter, linter, build, and test commands relevant to the change. Do not install tools or change dependencies unless required; report exact commands and limitations.
- Use the Rust Code Quality Reviewer for Rust implementation changes and the Rust Test Quality Reviewer when tests are added or changed. Keep reviews focused on changed code and behaviorally relevant consumers, and report actionable evidence and remaining risks.

## Copilot agents and skills

- When reviewing or changing files under `.github/agents/` or `.github/skills/`, check that each config clearly states its purpose, scope or triggers, workflow, constraints, applicable tool access, and expected output. Check related configs and workspace instructions together for consistent terminology, responsibilities, precedence, links, and tool expectations. Resolve contradictions when the intended rule is clear; otherwise, report the ambiguity and ask before encoding a precedence.
- If user requirements conflict with one another or applicable repository guidance, explicitly identify the conflict. If the user's intended precedence is unclear, ask before proceeding; if their request clearly resolves it, state the resolution and proceed. Never silently choose or claim incompatible requirements are all satisfied.

## Typst Documents

- All project-authored Typst documents, including review reports when produced, MUST use `#set par(justify: true)`, `#set page(margin: 40mm)`, `#set text(font: "Arial", size: 10pt)`, and `#set heading(numbering: "1.1")`.
- Include `#outline(title: [Contents], depth: 2)` after the title and metadata and before the main sections.
- Omit the page header on page one and center its Arabic page number in the footer.
- On later pages, center the latest outlined level-one heading on or before the current page in the header and put the Arabic page number at the right. Use `heading.where(level: 1, outlined: true)`, filter by `heading.location().page() <= here().page()`, and use equal flexible side columns around a content-sized center column so a heading does not wrap if it fits. The page number must remain visible even if no outlined heading has appeared.
- Compile new or changed Typst documents with `typst compile` when available. Report unavailable tooling or compilation failures explicitly, and keep generated PDFs out of the repository unless requested.
