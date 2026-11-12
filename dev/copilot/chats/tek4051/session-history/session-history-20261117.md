# Session history — 2026-11-17

## Adding a Flash Drive clear button

- Added a confirmed `Clear Flash Drive` button to the modular WASM browser shell. It disables when the directory is empty or another file operation is active; a declined confirmation leaves storage untouched.
- Added a shared clear operation through the tape, GPIB, machine, session, and WASM API layers. It removes all stored files and resets related tape selection/snapshot/cursor state; addressed transfers use the existing storage-replacement abort behavior. Clearing is rejected while a whole-store archive snapshot is active.
- Added a WASM API regression for clearing multiple files and the archive-snapshot guard, plus a Playwright regression covering empty-state disabling, cancellation, confirmation, row/count removal, and launch disabling. Updated the WASM guide.
- Validation passed: full native package tests, no-default-features WASM library tests, strict Clippy, formatting, JavaScript syntax, Typst compilation, WASM build and `wasm-bindgen 0.2.130`, `git diff --check`, and the focused browser clear test.
- Generated bindings/build outputs are Git-ignored under `target/`. The prior browser page ID did not resolve, so it was not reloaded to avoid changing its in-memory Flash Drive; reload it to see the new button.

## Running the WASM emulator

- Confirmed the workspace preview server and generated JavaScript/WASM bindings respond successfully.
- Opened a separate integrated-browser page and started the emulator. It reports Running at x1 and BASIC input possible; the Flash Drive is empty, so Clear Flash Drive is visible and disabled.
- Left the emulator running in page `86f9cf00-0a36-4881-95d0-32af3d64731f`.

## Reviewing Rust migration status

- Reviewed the current handoff and migration plans. The graded WASM browser parity plan records W0-W4 complete and W5 open; the broader migration guide's latest grade assessment is dated 14 November and still marks Grades 0-5 open, so overall sign-off remains unclaimed pending reconciliation with newer evidence.
- Open areas include freezing the Grade 0 baseline, physical native interaction and German Option+L, broader tape/GPIB storage and supplied-program coverage, option-ROM/peripheral coverage, and remote-terminal readiness semantics.
- The tracked worktree was clean at review time; `.npm-cache/`, `node_modules/`, and `target/` are Git-ignored.

## Updating all graded plans

- Refreshed the native migration guide's per-grade assessment with current Rust/native/WASM implementation and test evidence; all migration grades remain open pending their exit evidence.
- Updated the WASM parity plan to document the user-authorized Clear Flash Drive scope extension across the session API, browser UI, and W3-W5 gates. W0-W4 remain implemented/validated; W5 cross-browser/manual/release evidence remains open.
- Updated the architectural refactoring guide to record AR-0 through AR-2 evidence, later AR-3/AR-4 implementation evidence, and open integrated AR-3 through AR-5 acceptance. No architectural grade is claimed as formally accepted.
- Updated this handoff. All three Typst plans compile successfully, and `git diff --check` passes.
- The final worktree check showed a glossary deletion and untracked replacement/docs under `dev/docs`; the user clarified these are their changes. They remain untouched.

## Starting the Copilot glossary

- Initialized `dev/docs/copilot/copilot-glossary.typ` with the required Typst document conventions and a definition of “gate” as a criteria-and-evidence-based decision point.
- `typst compile --package-cache-path target/typst-packages dev/docs/copilot/copilot-glossary.typ /tmp/copilot-glossary-check.pdf` passed; the temporary PDF was removed.

## Adding the grade term

- Added “Grade” to the Copilot glossary, distinguishing a named plan stage and its exit criteria from the decision gate that evaluates completion.
- Typst compilation and `git diff --check` passed; the temporary PDF was removed.

## Adding plan and stage terms

- Added definitions of “Plan” and “Stage,” including how a plan describes intended work, how stages group work, and how grades and gates relate to stages.
- Typst compilation and `git diff --check` passed; the temporary PDF was removed.

## Adding entry and exit conditions

- Added definitions distinguishing stage prerequisites (entry conditions) from completion/transition criteria (exit conditions), and clarified that a gate evaluates evidence against criteria.
- Typst compilation and `git diff --check` passed; the temporary PDF was removed.

## Starting the Rust glossary

- Initialized `dev/docs/rust/rust-glossary.typ` with Cargo definitions for package, manifest, workspace, workspace manifest, and package manifest.
- Grounded the examples in the repository's virtual root workspace manifest and the `tek4051-emulator` package manifest.
- Typst compilation and `git diff --check` passed; the temporary PDF was removed.

## Adding the crate term

- Added “Crate,” distinguishing a Rust compilation unit and its crate root/module tree from a Cargo package, with this package's library and binary targets as examples.
- Typst compilation and `git diff --check` passed; the temporary PDF was removed.

## Adding module and test terms

- Added explanations of Rust modules, private modules, tests, and private tests. The private-test entry follows the repository convention of keeping private-access tests in separate files declared by private `#[cfg(test)]` modules, with public API tests in `tests/`.
- Typst compilation and `git diff --check` passed; the temporary PDF was removed.

## Adding dependency-direction and submodule terms

- Added explanations of downstream/upstream dependency relationships and Rust submodules, explicitly distinguishing a Rust submodule from a Git submodule.
- Typst compilation and `git diff --check` passed; the temporary PDF was removed.

## Adding the tighten term

- Added “Tighten” to the Copilot glossary, defining it as making a criterion more precise or restrictive while respecting scope and intended behavior.
- Typst compilation and `git diff --check` passed; the temporary PDF was removed.

## Adding requirement and validation terms

- Added explanations of requirement, rule, constraint, and check, distinguishing intended outcomes, governing directives, solution boundaries, and verification evidence.
- Typst compilation and `git diff --check` passed; the temporary PDF was removed.

## Adding the boundary term

- Added “Boundary” under Architecture and structure, defining a separation of responsibility/ownership and noting that folder layout alone does not ensure an effective boundary.
- Typst compilation and `git diff --check` passed; the temporary PDF was removed.

## Adding responsibility and ownership terms

- Added explanations of component, responsibility, ownership, and owner, distinguishing a system unit, its duties, and accountability/control.
- Clarified that architectural ownership is distinct from Rust's language-level memory ownership.
- Typst compilation and `git diff --check` passed; the temporary PDF was removed.

## Adding SOLID and pattern terms

- Added SOLID, pattern, anti-pattern, and gatekeeper entries to the Copilot glossary. The SOLID definition lists all five principles and frames them as heuristics; the gatekeeper entry covers criteria-based decisions.
- Typst compilation and `git diff --check` passed; the temporary PDF was removed.

## Correcting glossary spelling

- Corrected the term headings to “Gatekeeper” and “Anti-pattern,” and aligned the gatekeeper definition with the standard spelling.
- Typst compilation and `git diff --check` passed; the temporary PDF was removed.

## Expanding the SOLID explanation

- Expanded SOLID with a meaning and a common non-meaning for each of the five principles, and reiterated that they are context-dependent heuristics rather than a checklist.
- Typst compilation and `git diff --check` passed; the temporary PDF was removed.

## Running the native Rust emulator

- Ran `cargo run --locked -p tek4051-emulator` in the workspace terminal. Cargo finished the build and launched `target/debug/tek4051-emulator`; the running app inventory showed a separate window titled “Tektronix 4051 Emulator.”
- Computer Use could not bind to the app because `tek4051-emulator` is not installed as a recognized application. The emulator's paused/running state and display could not be inspected; no source files were changed.
- Confirmed the generated binary is Git-ignored by `/target/` in `.gitignore`.

## Investigating native catch-up skipped time

- Preserved the existing bounded catch-up policy: at most one 10 ms logical tick is advanced per worker iteration, with excess elapsed time recorded as skipped time.
- Changed display revision tracking so screen commands only invalidate the framebuffer revision when pixel values actually change. Added a public-API regression proving that X-port AlphaDot latching leaves the revision unchanged, the first Y-port draw advances it, and repeating the mark at the same coordinates leaves the revision unchanged.
- Reused the ignored native worker timing probe after the change. Post-change skipped time was 15.9%, 16.3%, 13.6%, 14.9%, 15.5%, 49.1%, and 64.7% at debug x1/x2/x5/x10/x20/x50/x100; release results were 19.4%, 18.2%, 18.5%, 17.5%, 16.8%, 17.2%, and 13.3%. Compared with the pre-change release baseline, these results do not show a broad improvement and do not reproduce the reported near-100% behavior. The worker harness is not a measurement of the user's exact GUI workload.
- The focused AlphaDot regression and full package suite passed. `cargo fmt --all -- --check`, strict all-target/all-feature Clippy, and `git diff --check` passed.
- Rust Code Quality review found no actionable issue. Rust Test Quality review requested a repeated-mark no-op assertion; it was added, the focused test passed again, and the reviewer confirmed the gap was resolved with no remaining findings.
- Generated build files remain under Git-ignored `target/`. The source and project-chat updates are uncommitted; live GUI performance remains unverified.

## User observation on 100x usability

- The user reports the emulator is perfectly usable at 100x. Its timer may rise around 50–60% per real second, which feels fast.
- This is user-reported usability evidence, not a new benchmark. The correspondence between the UI timer and the worker probe's skipped-time percentage remains unverified; no code or test changes were made.

## Preventing native key taps from being lost

- The user reported that key presses are lost in the Rust emulator. Native host presses/releases had been applied directly to the keyboard state; if both edges arrived before a worker tick, the release could hide the press before the ROM scanned it.
- Routed native host key actions through the existing input sequencer. The minimum press interval now preserves same-frame down/text/up taps through at least one ROM keyboard scan. Sequenced host presses preempt pending panel input when they occur later, and later panel presses retain their source priority; machine-readiness gating and its ignore-not-buffer policy remain unchanged.
- Added a GUI-adapter regression for a typed `1` down/text/up triplet in one frame and an end-to-end base-ROM regression confirming the ROM records it while the key remains held at 99 ms and the key releases at 100 ms.
- `cargo test --locked -p tek4051-emulator --quiet` passed: 125 library tests plus all package integration suites passed, with one ignored performance probe. The no-default-features library tests passed (22 tests). `cargo fmt --all -- --check`, strict all-target/all-feature Clippy, and `git diff --check` also passed. Both Rust reviewers reported no actionable findings.
- No live native keyboard session was exercised after the change; the ROM-level regression validates the native event adapter and machine path synthetically. Cargo outputs under Git-ignored `target/` remain ignored.

## Auditing render-pipeline boundaries

- Created [render-pipeline-audit.typ](../../../../docs/tek4051/render-pipeline-audit.typ) documenting the machine-owned rasterizer, public emulation snapshot, native egui preparation, WASM canvas conversion, and CPU screen-copy path.
- The audit finds a usable pixel-frame seam for an alternate presentation renderer, but no shared renderer plug-in interface. It recommends an explicit immutable frame contract and application-layer renderer adapters; it distinguishes a WebGPU presentation renderer from replacing machine rasterization and keeps screen captures sourced from CPU pixels.
- `typst compile dev/docs/tek4051/render-pipeline-audit.typ ...` and `git diff --check` passed. No emulator code changed and no WebGPU runtime or performance test was performed.
- Next implementation decision: choose browser WASM or native WebGPU, then define the frame metadata/ownership contract before adding renderer lifecycle APIs.

## Discussing renderer boundaries

- The user expects the renderer above `Machine`, in the emulation layer, and asked to discuss the boundary, performance, and complexity before settling the design.
- The current source places rasterization in `Machine::DisplayModel`; AR-3 separately assigns display conversion to an application presentation adapter. Clarifying emulation rendering versus frontend pixel/surface presentation is the remaining design question.
- No emulator or audit changes were made during this discussion; the audit's prior recommendation to keep rasterization in `Machine` is not accepted as a final architecture decision.

## Confirming emulation renderer ownership

- The user agreed that the machine-to-emulation boundary carries display operations/state, the emulation layer owns rendering, and frontends present the result. A CPU reference renderer must preserve the current rendering feature set; no observed behavior change is intended.
- Updated [render-pipeline-audit.typ](../../../../docs/tek4051/render-pipeline-audit.typ) to record the agreed target, distinguish emulation rendering from egui/browser presentation conversion, and require regression-level output parity. The WebGPU target/backend remains undecided.
- No emulator implementation changed. Typst compilation and `git diff --check` are pending.

## Moving display rendering into emulation

- Implemented the agreed boundary: `Machine` now emits ordered display operations, while `HostRuntime` applies them to an emulation-owned CPU reference renderer around instruction execution, phosphor refresh, and erase completion. The emulation snapshot, readiness detection, and event-time screen copies use this renderer; native egui and WASM remain presentation adapters. Removed the machine-owned framebuffer and rasterizer API.
- Migrated display, runtime, emulation, native, WASM, and FlashDriveTests consumers/tests. Updated the render-pipeline audit to describe the implemented boundary and remaining WebGPU/frame-contract work.
- Validation passed: `cargo test --locked -p tek4051-emulator --quiet`, `cargo test --locked -p tek4051-emulator --no-default-features --lib --quiet`, `cargo clippy --locked -p tek4051-emulator --all-targets --all-features -- -D warnings`, `cargo check --locked -p tek4051-emulator --target wasm32-unknown-unknown --no-default-features --features wasm`, `cargo fmt --all -- --check`, Typst compilation, and `git diff --check`. The FlashDriveTests session (7) and worker (5) suites passed; all interactive targets compiled and the separate package passed strict Clippy and formatting. Tests printed the existing `ZipWriter` drop warning while exiting successfully.
- Both focused Rust code/test reviews and their follow-ups found no actionable residual findings. The code review identified the external FlashDriveTests consumers; those were migrated and verified. Test review notes coverage represents important sequences but not every possible operation ordering.
- Confirmed Cargo build output under `target/` is Git-ignored. No WebGPU backend or runtime performance benchmark was performed. No commit was made. At final status, Rust source/tests were staged while the audit/chat files were unstaged; I did not change the staging state.

## Selecting the WASM WebGPU quality target

- The user clarified that WebGPU is intended to improve vector-display quality and may differ visually from the CPU reference. Compatibility applies to implemented emulation features, not pixel-perfect presentation.
- Selected browser behavior: prefer WebGPU when supported, fall back to the CPU reference renderer, and make screen copies match the active renderer. WebGPU should rasterize ordered display operations rather than merely upload the CPU bitmap.
- Updated the render-pipeline audit and handoff to distinguish behavioral compatibility from pixel parity. No WebGPU code was changed; device/surface lifecycle, capture readback, and runtime validation remain open.

## Creating the graded WebGPU renderer plan

- Created [graded-wasm-webgpu-renderer-plan.typ](../../../../docs/tek4051/web/design/graded-wasm-webgpu-renderer-plan.typ) with seven evidence-based grades, requirements mapping, validation gates, and explicit status that implementation has not started.
- Sequenced native Rust renderer work and acceptance before WASM integration. The plan retains the CPU reference and fallback, allows intentional pixel differences for quality, requires captures from the active renderer, and keeps machine display operations renderer-agnostic.
- Made WG-0 freeze the representative vector corpus and observable quality criterion; the exact threshold remains undecided rather than being invented. WG-1 verifies the GPU API/dependency and capture contract before implementation.
- `typst compile dev/docs/tek4051/web/design/graded-wasm-webgpu-renderer-plan.typ /tmp/graded-wasm-webgpu-renderer-plan.pdf` and `git diff --check` passed. The generated PDF is outside the repository. No Rust tests were run for this documentation-only task, and no WebGPU code or benchmark was added.
- Updated the handoff with the plan and the next action: establish WG-0 evidence, then complete native WG-1 through WG-3 before browser WG-4. The new Typst plan is not Git-ignored.

## Starting the native WebGPU vector prototype

- Added an optional emulation-layer offscreen vector pass. The host supplies `Arc<wgpu::Device>` and `Arc<wgpu::Queue>`; the renderer draws antialiased `VectorSegment` batches and supports RGBA readback for native validation. GPU adapter/device creation remains in the native test host.
- Tightened coverage assertions and tests for transparent pixels, both dimension limits, negative/zero widths, invalid intensities, and finite inputs that overflow coordinate deltas, segment length, or expanded corners. Renamed the dimension error to describe the actual device limit.
- This is still an exploratory slice, not an accepted WG grade: it does not yet consume ordered display operations, implement session phosphor/erase lifecycle, present to a browser surface, or provide active-renderer screen copies. WG-0's corpus and quality criterion remain unresolved.
- Validation passed: full package tests with `webgpu` (128 passed, 1 ignored), the explicitly invoked native GPU test, default-feature test compilation, WASM target check with `wasm webgpu`, formatting, strict all-target/all-feature Clippy, native build, and `git diff --check`. The native GPU test also byte-compares overlapping weak/strong vectors against the stronger-only frame and confirms an empty batch clears stale pixels. Cargo emitted the existing ZipWriter drop warning and `block v0.1.6` future-incompatibility notice.
- Both Rust reviewers found no remaining actionable findings. The native integration test checks both weak/strong segment orders against strong-only output and verifies that an empty batch clears stale pixels. Generated outputs under Git-ignored `target/` are ignored; changed source, tests, and docs are visible to Git.
- The user approved qualitative CPU/GPU side-by-side visual comparison at matching output sizes without a numeric threshold. Updated the graded plan, render-pipeline audit, and handoff, and compiled both Typst documents; WG-0 remains open until the representative corpus, CPU baselines, and review evidence are recorded.

## Integrating and launching native WebGPU

- Connected the emulation-owned WebGPU renderer to the native eframe WGPU backend. The app supplies eframe's device/queue, registers the renderer's offscreen texture for egui display, and keeps the CPU reference image as fallback. Initialization/render failures are surfaced, and GPU screen-copy readback errors now fall back to CPU pixels with a notice.
- The first launch exposed that the offscreen target lacked `TEXTURE_BINDING`, required when egui samples the registered texture. Added the usage flag and relaunched successfully. `cargo run --locked -p tek4051-emulator --features webgpu` remains running and the native window is listed; no worker activation failure was printed. Computer Use could not bind to this binary because it is not an installed app, so the active renderer and screen contents were not visually inspected.
- The WebGPU-feature package suite passed (128 library tests and all integration suites; one library test and the adapter test are ignored by default), as did the default package suite (125 library tests and all integration suites; one ignored). The high-speed cursor-readiness test initially exposed duplicate cursor events overflowing the extra WebGPU event queue; coalescing repeated cursor marks across operation batches fixed it. The explicitly invoked native GPU adapter test passed. Formatting, default `cargo check`, and strict all-target/all-feature Clippy passed. An existing `ZipWriter` drop warning and the `block v0.1.6` future-incompatibility notice remain.
- Rust Code Quality review found a screen-copy fallback gap; the implementation now retains the CPU capture and surfaces the GPU readback degradation. The focused reviewer follow-up found no remaining issue; the fallback is not directly fault-injected by a test. WG-0's representative corpus and qualitative matching-size comparison remain open; no grade is claimed.
- Rebuilt the final `--features webgpu` binary after the fallback refinement. The already-open process was not restarted, so it remains on the pre-refinement executable; restart it to exercise that rare screen-copy failure path. The current WGPU presentation path remains running.

## Confirming the native WebGPU emulator is running

- Rechecked the existing `cargo run --locked -p tek4051-emulator --features webgpu` process and native window; both remain running. Did not start a duplicate window. The existing process still predates the final screen-copy fallback refinement, while the latest binary is built on disk. Computer Use still cannot bind the executable to inspect its visible renderer state.

## Fixing native WebGPU input queue saturation

- The user reported that keyboard input was not echoed and the worker stopped with “emulation command queue full.” Reduced queue pressure by suppressing unchanged, actionless GUI input frames and dropping repeated keydown events before they produce modifier-only actions; focus, modifier, reset, and actual key transitions remain submitted.
- Reduced GPU submissions by staging stored vectors and composing once per ordered machine-operation batch. Screen-copy operations flush before capture, while `CompleteErase` discards staged vectors and clears persistent/transient layers in order.
- Added regressions for idle-frame coalescing with matching press/release identity, repeated-key suppression, explicit release retention while worker publication is stalled, and a native GPU screen-copy barrier that captures staged vector pixels. Strengthened the queue test to require the specific release action after reset.
- Validation passed: full WebGPU package suite (130 tests passed, 2 ignored), both native GPU adapter tests when explicitly invoked, targeted queue/input regressions, formatting, strict all-target/all-feature Clippy, WASM `webgpu` target check, and `git diff --check`. The WASM check reports the existing `MAX_PENDING_SCREEN_COPIES` dead-code warning; Cargo also reports the existing `block v0.1.6` future-incompatibility notice, and tests emit the existing ZipWriter drop warning.
- Rust Code Quality review found no actionable findings. Rust Test Quality follow-up found no remaining assertion gaps. After the user approved restart, rebuilt and launched `cargo run --locked -p tek4051-emulator --features webgpu`; the process is running. Visual keyboard echo and renderer state remain unverified because Computer Use cannot bind the executable. The generated `target/debug/tek4051-emulator` is Git-ignored by `.gitignore:2`.

## Investigating keyboard responsiveness and repeat program launch

- The user reported that after resetting, the second attempt to launch Battlestar Flash Drive file 2 ended with “Launch of file 2 cancelled; a partial command may remain in BASIC.”
- Traced Auto Load cancellation paths: window focus loss, Reset/Pause while typing, manual or panel input, and runtime failure. The exact trigger for this report is not yet known; cancellation behavior has not been changed and the running native app was not restarted.
- Tested 20/20 ms, 50/20 ms, and 100/20 ms manual host-key press/release intervals; each variant caused real-ROM BASIC readiness or text-entry regressions. Restored the existing 100 ms press/release pacing; the complete real-ROM and manual-input suites pass.
- Fixed a moved-value error in the new CPU-fallback cache regression by capturing the expected snapshot revision before ownership transfer.
- Validation passed: full WebGPU-enabled package suite (132 passed, 3 ignored), strict all-target WebGPU Clippy, `cargo check --locked -p tek4051-emulator --target wasm32-unknown-unknown --no-default-features --features wasm,webgpu`, `cargo fmt --all -- --check`, and `git diff --check`. WASM reports the existing `MAX_PENDING_SCREEN_COPIES` dead-code warning; tests emit the existing ZipWriter drop warning and Cargo reports the `block v0.1.6` future-incompatibility notice. Rust implementation review had no high-confidence findings; test review found no assertion gaps for the then-current timing experiment.
- Cargo-generated `target/` outputs, including `target/debug/tek4051-emulator`, are Git-ignored. The existing `dev/docs/tek4051/render-pipeline-audit.typ` worktree modification was left untouched.

## Speeding Flash Drive command typing

- The user observed that Flash Drive program launches type their generated BASIC commands slowly, with the cursor appearing to jump back and forth. Changed only `PressId::Script` pacing to 80 ms pressed and 50 ms released per character (130 ms vs. 200 ms); manual and panel key timing remains 100/100 ms because faster manual intervals failed real-ROM regressions.
- Updated sequencer boundary assertions. Both real-ROM native launch tests passed, including file-directory launch and Auto Load output comparison. The full WebGPU-enabled package suite passed (132 tests, 3 ignored); strict Clippy, WASM WebGPU check, formatting, and whitespace validation passed.
- This shared sequencer change applies to native and WASM program-load scripts. After the user approved a fresh native launch, ran `cargo run --locked -p tek4051-emulator --features webgpu` and verified `target/debug/tek4051-emulator` is running. Visual cursor behavior and measured launch speed remain unverified.
- The prior file-2 cancellation remains unexplained. The user is unsure whether focus or another interruption occurred; existing cancellation safety remains unchanged. If it recurs, identify its trigger before changing the cancellation policy.
- Rust Code Quality review found no issues with script-only pacing; Rust Test Quality review found the interval-boundary and real-ROM output assertions sufficient.

## Further keyboard responsiveness work

- The user reported that input remained very slow. A CPU-originated keyboard PIA data read was tested as an early-release acknowledgement, but shortened press/release phases caused real-ROM command-entry and readiness regressions; the speculative scan-acknowledgement implementation and tests were removed. Manual and panel pacing remains 100/100 ms.
- Reduced only scripted key timing to 50 ms pressed / 30 ms released (80 ms per character, versus the previous 130 ms). The full WebGPU-enabled package suite passed: 132 passed, 3 ignored, including real-ROM load/output tests. Strict all-target/all-feature Clippy, WASM WebGPU type-check, formatting, and `git diff --check` passed. The WASM check retains the existing `MAX_PENDING_SCREEN_COPIES` dead-code warning; Cargo reports the existing `block v0.1.6` future-incompatibility notice.
- Rust Code Quality and Rust Test Quality reviews found no actionable implementation or test-quality issues. The existing native WebGPU process was not restarted; the user should test the new build before any restart is attempted.

## Relating hardware notes to the Rust keyboard path

- Read the user-supplied [keyboard-input-2.adoc](../../../docs/tek4051/keyboad-input-2.adoc). Its proposed encoder/strobe → PIA → ROM ISR → RAM line buffer → Return sequence clarifies why the ROM's `0x0597`/`0x059c` key-state latch is not proof that BASIC consumed a character.
- In the current emulator, `Machine::keyboard_press` and `keyboard_release` overwrite the keyboard PIA input level; `HostRuntime` raises CA1 every 10 ms, and PIA data reads clear latched flags. The machine has no key FIFO. Existing ROM tests verify only the ISR-visible key-down/code state and release.
- Tried using that latch to pace queued keys, but real-ROM Auto Load and Flash Directory launch output tests regressed; removed the sequencer/session integration. The production timings remain manual/panel 100/100 ms and script 50/30 ms. A test-only latch observer and isolated machine test remain; no production behavior changed.
- Validation after rollback: `cargo test --locked -p tek4051-emulator --features webgpu --quiet` passed 133 tests with 3 ignored; strict all-target/all-feature Clippy, formatting, and the focused latch test passed. The native WebGPU process exited and was not restarted. Cargo artifacts under Git-ignored `target/` were generated by validation.
- Next: trace the ROM's PIA data read through its ISR into the line-buffer mutation and Return handoff, then identify a real BASIC-consumption acknowledgement before changing the sequencer again.

## Clarifying keyboard determinism and emulator timings

- The user clarified the delivery contract: every host keystroke must reach the machine-level keyboard path in the same way as input from a physically attached keyboard. The host must not decide based on BASIC readiness whether the event is delivered; subsequent processing is up to machine logic. This does not promise BASIC accepts or executes every key.
- Confirmed `10 ms` is an emulator cadence: JavaScript explicitly schedules the keyboard interrupt every 10 ms because an event-triggered interrupt had unacceptable emulation latency, and Rust uses the same interval. Rust's manual/panel `100 ms` press/release minima are sequencer policy, not JavaScript manual-key timing; JavaScript forwards physical keydown/up immediately. JavaScript function-key macros use a separate speed-dependent `kbsf` delay (35 ms default).
- The user further prioritized input correctness over readiness checks and asked to keep the first implementation straightforward. Removed the BASIC-prompt-readiness gate from running-session host input, retained readiness for status and program-launch features, and kept the existing native transport bounded by backpressuring the producer when both the worker channel and retry FIFO fill. The existing input sequencer and machine keyboard path remain unchanged; no new machine FIFO or acknowledgement was introduced.
- Updated native/WASM status text to say that prompt detection is status feedback rather than a keyboard-input gate. Added regressions for a pre-prompt PIA key, native keyboard interruption of Auto Load, and ordered input retention when the worker channel is full.
- Validation: `cargo test --locked -p tek4051-emulator --features "webgpu wasm" --quiet` passed (143 library tests and all integration targets; 3 ignored). Strict all-target/all-feature Clippy, `cargo fmt --all -- --check`, `git diff --check`, `cargo check --locked -p tek4051-emulator --lib --target wasm32-unknown-unknown --no-default-features --features "wasm webgpu"`, the focused WASM Playwright launch/status test, and `typst compile` for `docs/wasm.typ` passed. The WASM check reports the existing `MAX_PENDING_SCREEN_COPIES` dead-code warning; Cargo reports the existing `block v0.1.6` future-incompatibility notice.
- The user selected the focused-and-running scope: events routed to the emulator while running must reach the machine PIA regardless of BASIC readiness; stopped/unfocused events and focus-loss cleanup remain outside the guarantee. Delivery ends at the PIA; subsequent ROM/BASIC handling is machine logic. Cargo outputs under Git-ignored `target/` were generated by validation.

## Defining keyboard-delivery error conditions

- The user asked to avoid speculative fixes, define concrete error conditions, and ensure failures are caught and reported. Kept queue saturation as backpressure rather than treating it as an error or adding a timeout.
- Strengthened the saturation regression to signal from the full-retry-queue blocking path, bound its receive waits, and verify ordered matched press/release events. Added a GUI regression that verifies a worker command error is retained and displayed in the status area.
- Confirmed concrete error paths: worker disconnect/shutdown during submission returns a latched backend error surfaced by the GUI; snapshot errors use the same visible worker-error notice; machine runtime errors are shown as “Emulation stopped.” A live worker that ceases draining while still connected remains an indefinite backpressure wait; no watchdog threshold was assumed.
- Validation passed: full package tests (144 library tests passed, 3 ignored; all integration targets passed), strict all-target/all-feature Clippy, `cargo fmt --all -- --check`, `git diff --check`, and the WASM library target check. The WASM check retains its existing dead-code warning, and Cargo reports the existing `block v0.1.6` future-incompatibility notice.
- The user confirmed that a live-but-non-draining worker should remain ordinary backpressure in this first implementation; no watchdog or arbitrary timeout will be added.
- Build output under Git-ignored `target/` was generated. Next: user testing of keyboard behavior remains useful; any future stall detector requires a defined objective signal and policy.

## Comparing keyboard input at x1 and x100

- The user reports interactive input is fast and no keys appeared missing at x1, but interactive input is unusably slow at x100. ASCII programs launched from the Flash Drive view still run fast; generated `FIND`/`OLD` commands are as slow as interactive typing.
- Verified that manual input and scripted program-load commands share `InputSequence`; manual press/release minimums are 100/100 ms, while script minimums are 50/30 ms. The sequence advances with host elapsed time and calls `HostRuntime::advance`; CPU instructions per interval scale with the selected speed factor.
- This establishes a plausible coupling between serialized input pacing and x100 worker CPU load, but does not yet identify the cause. Next: measure host-event-to-PIA latency and worker tick cost at x1 and x100 before changing pacing or queue behavior. No code change was made for this observation.

## Decoupling host key capture from emulation consumption

- The user clarified the producer/consumer contract: the host should enqueue key transitions as quickly as events arrive, while the emulation worker consumes the same ordered FIFO independently at its own processing rate.
- Replaced the GUI-owned retry queue plus worker channel with one bounded, shared FIFO consumed directly by the existing worker. Host enqueue only holds the queue lock for a short append; the worker pops commands outside machine processing and retains the existing command order.
- The FIFO accepts up to 1,024 key transitions and reserves one extra command slot for recovery. Overflow rejects the newest frame, queues an unfocused input-release frame after already accepted events, latches a visible error, blocks subsequent keyboard input, and keeps non-input controls available. Worker health is now tracked separately so a queue overflow does not disable Run/Pause or Reset.
- Added regressions for producer completion while the consumer is stalled, exact transition capacity, FIFO press/release order, overflow latching/recovery, visible input-overflow reporting, and idle-frame coalescing.
- Validation: default library tests passed (131 passed, 1 ignored); full WebGPU-enabled package tests passed (134 library tests passed, 3 ignored, and all integration targets); strict all-target/all-feature Clippy, workspace formatting, and staged/unstaged whitespace checks passed. Cargo emitted the existing `block v0.1.6` future-incompatibility notice. The passing suite also printed a `ZipWriter` write-zero diagnostic; its source was not investigated.
- The worker remains responsible for existing input sequencing and machine behavior; no BASIC-readiness gate, timing change, or machine-level acknowledgement was added. Next: re-test interactive typing and `FIND`/`OLD` at x1 and x100; measure latency before changing pacing.
- The earlier detached WebGPU launch completed with exit code 0 during this chat; it was not restarted.
- Both required Rust reviews completed follow-ups with no remaining findings; a default-feature focused queue test also passed after tightening the bounded consumer-test timeout and recovery-order assertions.
