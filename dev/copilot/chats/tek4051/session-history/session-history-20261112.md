# Session history - 2026-11-12

## Initial Copilot setup

- Added project-independent Rust code-quality and test-quality reviewer agents, workspace instructions, and Rust implementation/test comment skills.
- Added the reusable setup guide at `dev/copilot/setup/setup.adoc` and started the project handoff at `dev/copilot/chats/tek4051/tek4051-handoff.md`.
- Confirmed the planned direction is a Rust port using egui. Inherited upstream files must remain unchanged unless the user specifically authorizes a change.
- Agent and skill metadata, documentation links, whitespace, and setup-guide AsciiDoc rendering were checked during setup. The Rust port has not started.

## Starting daily chat history

- Created this initial history file using the `session-history-YYYYMMDD.md` naming pattern.
- Updated workspace instructions to read the handoff and today's history at chat start, append a factual summary after each chat, create new daily files as needed, and keep the handoff current.
- No emulator source or assets were changed. The next implementation-planning action remains an inventory of the existing emulator's behavior and dependencies.

## Rust setup companion

- Added `dev/copilot/setup/setup-rust.adoc`, adapting general Rust/Cargo setup guidance for the planned egui port without importing Bevy or Asteroids architecture.
- Linked the companion from the general setup guide and handoff. Documented upstream-file protection, dependency and layout decisions still pending, scoped Cargo validation, runtime checks, and test/reviewer conventions.
- Both setup guides rendered successfully with Asciidoctor; documentation links and whitespace checks passed. The new guide is not ignored by Git. Cargo checks were not run because no Rust implementation was added.
- This was documentation-only work; no Cargo package, dependencies, emulator source, or assets were changed. The next action remains inventorying existing behavior before selecting the first increment.

## Typst workspace conventions

- Copied portable Typst instructions into `.github/copilot-instructions.md`: justified paragraphs, 40 mm margins, 10 pt Arial, numbered headings, a depth-two contents outline, first-page footer numbering, and later-page running headings with right-aligned numbering.
- Added compilation and generated-PDF handling guidance without importing Asteroids document paths or mandatory review-report requirements.
- Updated the handoff. No Typst document or upstream emulator file was changed.

## Reviewing project guidance

- Read `.github/copilot-instructions.md` and `dev/copilot/chats/tek4051/tek4051-handoff.md` as requested.
- No implementation, project decision, or validation state changed; the next action remains inventorying existing emulator behavior and dependencies.

## Web emulator design and graded migration guide

- Reviewed the requested web emulator source under `dev/src/4051-Emulator/web/4051-Emulator`, including the HTML entry point, CPU/machine, display, keyboard, ROM data, GPIB devices, storage codecs/UI, stylesheet, and package/test metadata.
- Added `dev/docs/tek4051/web/design/design.typ` with the source architecture, observable behavior, proposed Rust/egui boundaries, parity evidence, and unresolved platform/storage decisions.
- Added `dev/docs/tek4051/web/design/graded-migration-guide.typ` with Grade 0 baseline readiness through Grade 5 full parity sign-off, each with implementation scope and exit evidence.
- Recorded that the source README claims browser-local storage but the inspected storage code is an in-memory file store with no browser persistence API; runtime baseline must resolve the discrepancy. Also preserved the documented R12 MUSIC/SOUND limitation as out of parity scope.
- Both Typst documents compiled successfully (`typst compile`); generated check PDFs were temporary and removed. The requested docs are not ignored by Git and `git diff --check` passed.
- No emulator source or supplied assets were changed. A zero-byte, untracked root `Cargo.toml` was observed and left untouched; no Rust package exists yet.
- Next: run the existing web implementation against the Grade 0 checklist and confirm platform and storage persistence expectations before creating the Rust package.

## Rust project paths clarified

- Updated the design and graded migration guide to use the repository-root `Cargo.toml` as the manifest path and `dev/src/4051-Emulator/rs/4051-Emulator-rs` as the implementation root.
- The root manifest remains zero bytes and untracked; it was not edited. Crate/module structure, target platform, and storage persistence remain open decisions.
- Updated the handoff so the established paths are distinguished from the still-undecided Cargo package configuration.

## Migration plan

- Added `dev/docs/tek4051/web/design/migration-plan.typ`, sequencing baseline capture, Cargo/core setup, display and keyboard, GPIB/storage, extended ROM/peripheral parity, and final sign-off with dependencies and exit gates.
- Updated the plan to use the repository-root `Cargo.toml` and the established Rust implementation root. A later check found `Cargo.toml` tracked as a zero-byte file; it was not modified.
- The migration plan compiled successfully with `typst compile` as a six-page PDF; the temporary PDF was removed. Whitespace checks passed and the plan is not ignored by Git.
- No emulator source, assets, Cargo manifest, or staged changes were modified. Next: execute the plan's baseline phase, resolve target platform and persistence, then configure the Cargo package.

## Starting the Rust migration

- The user selected native desktop and chose to match the web reference's apparent in-memory tape behavior.
- Served `dev/src/4051-Emulator/web/4051-Emulator/Tektronix4051.html` locally. The page loaded, Start enabled the emulator controls, and Reset responded; the browser reported one malformed select/input markup warning. No program or peripheral parity corpus has been captured yet.
- Configured the repository-root Cargo package with its library source under `dev/src/4051-Emulator/rs/4051-Emulator-rs`, added Rust build-output ignores, and implemented a testable MC6800 slice: reset vector, NOP/zero-as-NOP, immediate LDAA and ADDA, and extended STAA, with explicit bus/illegal-opcode errors and focused tests.
- `cargo fmt --all -- --check`, `cargo check --locked`, `cargo build --locked`, `cargo test --locked` (10 passed), and `cargo clippy --locked --all-targets -- -D warnings` pass. All three Typst design/migration documents compile; temporary PDFs were removed.
- The Rust code reviewer reported no actionable findings. The test reviewer identified assertion and error-path gaps; those were addressed (including both NOP encodings, both LDAA zero-flag transitions, STAA flag cases, reset-vector read failures, and interrupt-mask preservation), then the tests passed and the reviewer confirmed no remaining high-confidence gaps.
- Cargo-generated `/target/` outputs are intentionally ignored by the new root `.gitignore`; Cargo files and Rust source remain visible to Git. Native UI, full CPU/board, ROM boot, and the Grade 0 program/peripheral corpus are still future work.

## Running the shared browser reference

- Checked the shared `Tektronix4051.html` page. The emulator is already running at x1 (Start disabled, Pause enabled); no additional click was needed. This is the JavaScript reference, not a Rust UI.

## Continuing the MC6800 core

- Continued validation of the Rust CPU expansion in `dev/src/4051-Emulator/rs/4051-Emulator-rs/cpu.rs`. Added focused coverage for signed overflow in A/B compare and subtract-with-carry, B ADC carry-in and load flags, logical-operation H/C preservation and V clearing, combined branch flags and CCR preservation, signed displacement endpoints, and BSR/RTS success and bus-error paths.
- Fixed RTS stack reads so SP changes only after successful reads and is restored if the second byte of an RTS return-address read fails. Corrected the branch wraparound fixture to avoid overlap with the reset vector.
- `cargo fmt --all -- --check`, `cargo check --locked`, `cargo build --locked`, `cargo test --locked` (24 passed), `cargo clippy --locked --all-targets -- -D warnings`, and `git diff --check` pass. The design and migration plan Typst documents compile. The code reviewer confirmed the stack fix; the test reviewer found no remaining high-confidence gaps.
- No web source or assets were changed. Cargo's `/target/` output remains intentionally Git-ignored; the Rust source and project notes are visible to Git.
- Next: continue direct/indexed/extended addressing and instruction coverage toward base-ROM boot, then implement the 4051 memory map. The Grade 0 program/peripheral baseline and native egui app remain incomplete.

## Migration log

- Created `dev/docs/tek4051/migration-log.typ` to consolidate the migration's decisions, issue/fix register, dated milestones, validation evidence, and open work. It records the native desktop and in-memory storage decisions, source-protection constraint, observed browser warning and baseline gaps, CPU review fixes, and current boot/parity blockers.
- The Typst log compiled successfully with `typst compile`; its temporary PDF was removed. The handoff now links the log. No emulator source or supplied assets were changed.

## Running the Rust port

- The user clarified that “run Tek 4051” meant the Rust port. `cargo run --locked` confirms there is no binary target yet; the package currently exposes only the CPU library, and the native egui application has not been implemented.
- `cargo test --locked` passes all 24 focused CPU tests. A local server was briefly used to load the JavaScript reference, then stopped after the clarification; that browser page is not the Rust emulator. No emulator source or assets were changed.
- Next: implement a native executable/egui shell after the CPU and machine foundation is ready.

## Phase 1 machine-core increment

- Implemented direct/indexed/extended byte operations, selected 16-bit word and unary operations, stack/control operations, SWI/RTI, and explicit IRQ/NMI service. Added a 32 KB RAM/base-ROM memory map, PIA DDR/output/input muxing, bank-selection state, and a bounded instruction-step API.
- Generated `base_rom.bin` under the Rust implementation root from the 16 existing 2 KB base-ROM chip arrays; upstream web code and supplied assets remain unchanged. Verified 32 KB length and SHA-256 `5b847c350fbd2e19fb609070bf76a929ce4068292a4c6cdc8568e91b2a4df59e`. The machine executes the first twelve ROM reset instructions, but full startup is not yet supported.
- Review found that the web source's NEG carry and CPX overflow behavior differs from conventional MC6800 descriptions. Compared with `mc6800.js` and intentionally preserved its behavior for parity, with regression tests. Expansion-ROM reads now return an explicit unsupported-bank bus error instead of silently using base ROM data.
- Validation passes: `cargo fmt --all`, `cargo test --locked` (39 passed), `cargo check --locked`, `cargo build --locked`, `cargo clippy --locked --all-targets -- -D warnings`, `git diff --check`, and `shasum -a 256 .../base_rom.bin`. The design, graded guide, migration plan, and migration log all compile with `typst compile`.
- The `base_rom.bin` and Rust source are not Git-ignored; Cargo's `/target/` build output remains ignored. No egui app/binary, complete CPU, PIA interrupt/keyboard behavior, expansion ROM integration, or Grade 0 parity corpus exists yet.
- Next: continue CPU implementation through full base-ROM startup, finish board input/interrupt signaling, and capture the reference program/peripheral baseline before any parity claim.

## Partial Phase 0 baseline capture

- Served the protected web reference at `http://127.0.0.1:8765/Tektronix4051.html` from source revision `b33e20fb3ef41a1db114b602a29a9ba8ce3fdb48`; HTTP returned 200. The browser displayed its existing malformed select/input markup warning.
- Verified the idle/start/pause/reset lifecycle, settings cycles (speed, scaling, granularity, mute label), default GPIB devices, printer add/remove, and duplicate-address no-op. Reset cleared the 1046×830 canvas to 0 non-black pixels.
- Imported the supplied `prog.txt` through Storage, saved it as ASCII/Program file 1 with description `Vector graphics demo`, selected it, and used Auto Load. The 12-point complete-graph drawing produced 58,148 non-black/green pixels with bounds x=16..837, y=37..742. With an empty tape store, Auto Load displayed `NO PROGRAM FOUND - MESSAGE NUMBER 59`.
- Reloading the page cleared the saved tape file, confirming the selected in-memory storage behavior. Importing a zero-byte file left no file entry and showed no visible error. The `tek_load()` helper references a nonexistent `#loadfile` element; `#importObj` is the tested import path.
- Added an unfrozen draft baseline under the Rust package's `tests/test-cases/`, including a byte-identical program fixture, zero-byte fixture, expected control/display captures, nine cases, an empty dependency table, and a summary under `dev/docs/tek4051/phase-0`. The user deferred the environment-mode choice; `mock` is recorded provisionally and may be changed before freeze.
- `npm test` was run and failed with the package's placeholder message `Error: no test specified` (exit 1). No upstream web source or assets were changed.
- The Phase 0 gate remains open: printer output, physical host-key input, complete display-mode comparisons, and GPIB byte/EOI transcripts are not yet captured. Next: finish those observations, revalidate the spec, and freeze only after the remaining parity evidence is recorded.
- Re-ran the supplied vector program at x100 under all three display-granularity labels and recorded raw RGB pixel counts in `tests/test-cases/testdata/expectations/display-granularity.json`; `⊚` yielded 58,148 non-black pixels, while `⊙` and `⊛` each yielded 24,973. The vector-only input does not distinguish the third mode's character sizing. A temporary printer probe showed no output, and a physical-key attempt was inconclusive; GPIB byte/EOI evidence remains open. Updated the draft case inventory, summary, migration plan/log, handoff, and left Phase 0 unfrozen. No upstream web files or assets were changed.

## Continuing migration: base-ROM startup loop

- Traced the base-ROM reset path past its RAM-clear loop. With default PIA input, the machine reaches a repeatable loop at `0xC6B4` within 100,000 instructions; the full CPU register state is unchanged after another twelve instructions. No unsupported opcode was encountered in this bounded run.
- Added `base_rom_completes_startup_and_repeats_its_monitor_loop` to `machine.rs`. The Rust Test Quality Reviewer found no actionable issues; it noted that the test verifies the steady loop rather than additional individual startup side effects.
- `cargo fmt --all -- --check`, `cargo check --locked`, `cargo build --locked`, `cargo test --locked` (40 passed), `cargo clippy --locked --all-targets -- -D warnings`, and `git diff --check` all passed.
- Updated the migration plan, migration log, and handoff. No upstream web code/assets were changed.
- Next: implement and test PIA/keyboard input and interrupt behavior against the monitor loop; separately complete the outstanding Phase 0 reference captures before freezing that baseline.

## Benchmark browser autoplay

- Added the Benchmark npm workspace under `dev/src/4051-Emulator/web/playwright/Benchmark`, using the shared root Playwright dependency, headless/headed runs, an interactive UI command, stage screenshots, actual tape-operation traces, and saved timing results.
- Extracted SimpleDemo's startup, import, keyboard, storage observation, and browser/server configuration into `web/playwright/support`; kept its five existing scenarios.
- The user selected unchanged-reference behavior. Program 2 produces all seven measurements in browser-memory file 10, then repeatedly reads absent file 51 and remains BUSY; the test explicitly reproduces this limitation rather than claiming normal completion. Program 4 is explicitly skipped because its MFM `!TIME` ROM is unavailable.
- `npm test` exited 0: Benchmark 5 passed/1 skipped; SimpleDemo 5 passed. The targeted speed-sweep command also passed. Verified 3200 x 2400 screenshots, the calculation program's DONE/X output visually, shared workspace links, no per-suite node_modules, and clean whitespace.
- No emulator, supplied Benchmark/FlashDrive asset, dependency-version, or account/global setting changes. Generated screenshots, tape/result evidence, dependency/cache files, and the validation report are intentionally Git-ignored.
- Next: use the new autoplay as additional web-reference evidence; any correction of the supplied file-51 readback or support for the MFM clock requires a separate authorized change. Rust implementation and Phase 0 freeze status are unchanged.

## Benchmark analysis document

- Added `dev/src/4051-Emulator/web/playwright/Benchmark/Benchmark-analysis.typ` and linked it from the Benchmark README.
- Documented all seven tape assets, selector/calculation/TLIST flows, seven-speed timing and saved evidence, shared autoplay synchronization, and coverage limits. Distinguished the observed file-51 readback loop from the declared, unexecuted MFM clock limitation and explained why `CALL "!TIME"` is not interchangeable with GPIB `INPUT @2:`.
- Compiled successfully with `typst compile` using the required typography, contents outline, and running header/footer conventions. Checked outlined sections, whitespace, and Git visibility; removed the temporary check PDF. The Typst source is not ignored.
- Documentation-only: browser tests were not rerun; the document explicitly labels the previously recorded 10 passed/1 skipped results. Emulator, supplied assets, test implementation, and runtime policy are unchanged.

## Battlestar browser autoplay

- Clarified that the requested destination means new autoplay support, not moving the source assets. Added the Battlestar npm workspace under `web/playwright/FlashDrive/Battlestar`, using shared root dependencies/helpers and root run/UI commands.
- Inventoried 91 files (24 programs, 66 data files, one LAST marker). Tests verify import and large-binary integrity; run picture generators 2, 26, 29 and converter 8; start game 4 with UDKs and exit with UDK10; and page TLIST through the complete directory.
- An actual load of main demo 1 fails on Option 30 `ROPEN` at BASIC line 3490. The user explicitly selected unchanged-reference behavior again. Main demo 1, Option 30 game 6, and MUSIC utility 28 are explicit compatibility skips; 15 other tools/setup programs remain imported but unexecuted. Navigation back to file 1 is recorded as a known load barrier, not a restored menu.
- Fixed the game screenshot wait to reject the transient solid-green PAGE erase flash. Visually verified the resulting targeting/Cylon image, tanker text, directory pages, and the original-demo syntax error.
- `npm test` exited 0: Battlestar 7 passed/3 skipped, Benchmark 5 passed/1 skipped, SimpleDemo 5 passed. Syntax/whitespace checks passed, PNG dimensions are 3200 x 2400, and no per-suite dependency directories or protected-source differences were found.
- No emulator, supplied program/data asset, dependency-version, or account/global setting changes. Generated screenshots, traces, write counts, and workspace-cache validation reports are intentionally Git-ignored.
- Next: use the supported Battlestar scenarios as web-reference evidence. Main-demo compatibility work and broader two-drive/utility coverage are separate follow-ups; do not imply complete Battlestar or Rust feature parity.
