# Session history — 2026-11-19

## Comparing WASM WebGPU and CPU-fallback profiles

- Continued the browser-only WebGPU investigation without changing Rust sources, Cargo manifests, or the Rust/WASM interface.
- Extended the opt-in Playwright profiling test to capture both WebGPU and forced CPU-fallback profiles in one run, keeping their generated output directories distinct.
- Captured five-second headed Chrome profiles at the x100 BASIC prompt/cursor. WebGPU sampled `HostRuntime::sync_display` 2,357 times, `WebGpuVectorRenderer::record_segments` 2,083 times, and `segment_instance_bytes` 1,293 times. CPU fallback sampled `HostRuntime::sync_display` 379 times, `HostRuntime::apply_display_operation` 374 times, and `CpuReferenceRenderer::apply_screen_command` 148 times.
- The inclusive sample counts overlap between parent and child frames; do not sum them or treat sampled milliseconds as elapsed-time comparisons. The profile comparison localizes substantial additional WASM-side work to WebGPU segment preparation/serialization, but does not prove that transient cursor geometry alone causes the slowdown.
- Updated the graded WASM WebGPU plan and handoff with the profile evidence, limitations, and a paired-profile command. Generated `.cpuprofile` and summary files are under Git-ignored `target/browser-input-tests/`.
- Validation passed: both profile cases (2/2), focused headed input/layout/renderer suite (23/23), `node --check performance.spec.mjs`, Typst compilation, and `git diff --check`.
- Next: keep browser-only scope; request separate authorization before any Rust-side profiling or optimization. Broader lifecycle acceptance remains open.

## Separately authorized Rust transient-geometry cache optimization

- The user separately authorized a Rust-side profiling/optimization pass; the browser-only graded plan remains otherwise unchanged.
- Cached packed transient segment instance bytes incrementally in `emulation/webgpu.rs`, retaining a segment count rather than a duplicate `VectorSegment` list. Refresh and complete erase clear the count and packed bytes together; redraw order and GPU buffer creation/upload behavior remain unchanged. `PendingDisplayGeometry` stages stored/transient segments and erase/dirty flags until packing succeeds, so rejected appends do not partially mutate renderer state.
- Three paired headed Chrome 158 benchmark runs at 1280x720 produced BASIC prompt/cursor medians of 138.84% / 102.17% of the 10 ms budget (WebGPU / CPU fallback) at x20, 340.43% / 250.62% at x50, and 677.23% / 495.77% at x100. Relative to prior WebGPU medians (229.22%, 574.02%, and 1119.15%), the WebGPU cost decreased about 39-41%. Vector-loop results remained near their earlier values. These are host-specific measurements, not thresholds.
- The final paired five-second x100 profiles sampled WebGPU `HostRuntime::sync_display` 1,290 times, `PendingDisplayGeometry::commit` 810 times, `TransientGeometry::append` 742 times, and `append_transient_segment_bytes` 708 times. CPU fallback sampled `HostRuntime::sync_display` 380 times, `HostRuntime::apply_display_operation` 363 times, and `CpuReferenceRenderer::apply_screen_command` 164 times. Earlier full-list serialization/segment-recording frames were no longer among leading WebGPU frames. Counts are inclusive and descriptive, not additive elapsed-time measures.
- The WebGPU library suite passed (154 passed, 7 ignored), strict all-target WebGPU Clippy and formatting passed, WASM build and binding generation succeeded, and paired profile tests passed (2/2). The ignored adapter-backed CompleteErase and refresh lifecycle tests were run explicitly and passed. Cargo emitted the pre-existing `block v0.1.6` future-incompatibility notice; the library run printed a `ZipWriter` `WriteZero` diagnostic after the successful test result. Generated build and profile outputs are under Git-ignored `target/`.
- Remaining x100 WebGPU workload is still above CPU fallback; next, measure redraw buffer creation/upload separately before any further renderer change.

## Measuring WASM WebGPU buffer costs

- Added opt-in page-side metrics for segment-instance `GPUDevice.createBuffer`, mapped initialization through `GPUBuffer.unmap`, synchronous `GPUQueue.submit`, and a post-pause `queue.onSubmittedWorkDone()` drain. The metric window records the actual reset-to-pause interval; the existing CPU-profiler tests remain separate to avoid profiler overhead in these timings.
- Three headed Chrome 158 runs at 1280x720 measured 5.62-5.67 seconds each. Medians [ranges]: 83 segment-buffer creations [82, 84], 133,317,760 bytes [131,242,816, 137,032,448], 107 queue submissions [106, 108], `createBuffer` 2.2 ms [1.1, 2.5], mapped initialization through unmap 88.5 ms [88.1, 88.9], and `queue.submit` 1.2 ms [0.8, 1.8].
- Combined synchronous host-side buffer setup was about 1.6% of each measurement window, unlikely by itself to explain the remaining x100 advance gap. The post-pause GPU queue-completion wait had a median of 13.0 ms [9.0, 14.4]; this is one tail wait per run, not a per-frame GPU timing, and does not localize GPU execution.
- The standalone metric test passed 3/3, and JavaScript syntax checks passed. Outputs are under Git-ignored `target/browser-input-tests/`.
- Next: profile/measure append-time geometry construction and validation; keep asynchronous GPU completion distinct from synchronous `emulator.advance()` workload.

## Assessing buffer pooling for cursor updates

- Discussed whether to pool GPU buffers for frequently changing geometry such as the blinking cursor. The renderer already caches transient segment bytes on the CPU, but creates a fresh GPU vertex buffer for each nonempty redraw.
- The measured `createBuffer` host time was only about 2.2 ms per 5.6-second window, while mapped initialization was about 88.5 ms. Pooling can reduce object/allocation churn but cannot by itself eliminate those data uploads or GPU draw work; the x100 capture also uploaded a large aggregate volume, so it was not just a tiny cursor-buffer workload.
- Recommendation: profile before implementation. If that justifies a change, compare capacity-reused storage with separating the small cursor overlay from the larger transient geometry; keep GPU queue ordering/in-flight buffer use correct and validate actual workload plus presentation.

## Profiling WASM geometry and evaluating cursor debounce

- Captured paired five-second headed Chrome x100 CPU profiles (2/2 passed). WebGPU inclusive sampled time: `HostRuntime::sync_display` 1,588 ms; `WebGpuVectorRenderer::apply_display_operation` 1,095 ms; `PendingDisplayGeometry::commit` 993 ms; `TransientGeometry::append` 921 ms; `append_transient_segment_bytes` 879 ms; and `validate_segment` 278 ms. These are nested inclusive samples, not additive durations or elapsed-time comparisons. The CPU-fallback profile sampled `HostRuntime::sync_display` at 467 ms, `HostRuntime::apply_display_operation` at 449 ms, and `CpuReferenceRenderer::apply_screen_command` at 211 ms.
- Self-sample inspection found 64 ms in `append_transient_segment_bytes`, 39 ms in `validate_segment` plus additional finite-check iterator frames, and 60 ms in `Vec<u8>::spec_extend`; these leaf samples identify serialization/validation work but do not by themselves establish the benefit of a specific rewrite.
- The 100 ms phosphor refresh is distinct from cursor event delivery. A WebGPU-only set keyed by mark position and intensity safely suppresses exact repeats until refresh/erase: max blending makes duplicate output idempotent, while moved positions and changed intensity remain drawable. Duplicate-only batches do not dirty/resubmit the GPU frame. CPU reference output, BASIC readiness events, and screen-copy/erase ordering are unchanged.
- One headed Chrome 158 run at 1280x720 compared the deduplicated build with an un-deduplicated build. BASIC prompt/cursor budget use fell from 690.33% to 535.71% at x100, 348.91% to 269.76% at x50, and 141.84% to 109.89% at x20 (about 22% at each); vector-loop x100 was 572.21% vs 560.06%. A temporary build skipping all WebGPU `TransientCursorMark` drawing after deduplication measured x100 BASIC at 528.26%, only 1.4% below deduplication; vector-loop x100 was 560.09%. These single-run measurements are directional and host-specific.
- Kept per-refresh duplicate suppression; no further cursor debounce is justified yet. Full WebGPU-feature package tests, strict Clippy, WASM check/build and binding generation, formatting, four adapter-backed tests, and both Rust reviewer agents passed. Next: investigate other transient geometry or GPU queue contention. Generated profiles/build outputs are under Git-ignored `target/`.

## Reprofiling and measuring uploads after cursor deduplication

- Captured fresh paired five-second x100 WebGPU/CPU-fallback profiles (2/2 passed). Current WebGPU inclusive samples were `HostRuntime::sync_display` 817 ms; runtime `apply_display_operation` 807 ms; `WebGpuVectorRenderer::apply_display_operation` 354 ms; `PendingDisplayGeometry::commit` 180 ms; `TransientGeometry::append` 65 ms; `append_transient_segment_bytes` 31 ms; and `validate_segment` 1.3 ms. Before deduplication, the latter three were 921 ms, 879 ms, and 278 ms, respectively. These nested inclusive profile times are descriptive and not additive/elapsed-time comparisons.
- The current profile's major inclusive frames were CPU execution: `Machine::run_steps` 3,877 ms and `Machine::step` 3,783 ms. CPU fallback showed 4,232 ms and 4,114 ms respectively. The remaining high x100 workload is therefore largely shared machine execution, not WebGPU segment packing.
- Three headed WebGPU buffer measurements at 1280x720 (5.50-5.53 s windows) recorded 10 segment buffers / 25,600 bytes each, mapped initialization median 0.3 ms, queue submissions 40 each, and post-pause queue drain median 0.6 ms [0.4, 0.9]. Pre-dedup medians were 83 buffers / 133,317,760 bytes, 88.5 ms mapped initialization, 107 submissions, and 13.0 ms [9.0, 14.4] drain. The current upload footprint is tiny; this evidence does not justify buffer pooling or further cursor throttling.
- Next: avoid additional WebGPU geometry changes absent a new hotspot; request scope direction before optimizing the shared 6800 CPU execution path. Generated artifacts remain under Git-ignored `target/`.

## Optimizing shared CPU execution

- User selected profiling and optimization of shared CPU execution, with the constraint that added complexity must have justified value. The WebGPU profile was unavailable in this browser, so used the forced CPU-fallback WASM path; a five-second x100 profile showed inclusive samples in `irq_pending` iterator closures (575 ms) and `nmi_pending` iterator closures (448 ms).
- Replaced the fixed-size PIA `.into_iter().any(...)` scans with direct short-circuit OR checks over the same PIAs and in the same order. The post-change profile no longer lists those iterator closures among its leading frames.
- Routed instruction decoding by opcode high nibble to avoid probing unrelated decoder families. Existing CPU tests and a focused Rust Code Quality review found no issues.
- Paired three-run CPU-fallback WASM benchmarks (headless Chrome 158, 1280x720): before changes, x100 BASIC prompt/cursor median was 485.51% of the 10 ms emulation budget and vector-loop median was 542.75%. After both changes, medians were 250.98% and 313.88%, reductions of 48.3% and 42.2%. After only direct PIA checks, medians were 274.03% and 340.99%; dispatch added 8.4% and 8.0% further reductions. These are host-specific emulation-call measurements, not total browser CPU percentages or release thresholds.
- Validation: 146 library tests passed (1 ignored), 25 machine integration tests, 32 CPU integration tests, 7 emulation integration tests, formatting, strict library Clippy, WASM build/binding generation, and focused code reviews. The profile test passed on CPU fallback; WebGPU profile was skipped because WebGPU was unavailable. The matching `wasm-bindgen-cli` 0.2.130 was installed temporarily for binding generation and then uninstalled. All generated outputs are under Git-ignored `target/`.
- Next: investigate remaining bus/display frames only after a fresh profile and matched benchmark show a worthwhile, low-complexity change.

## Optimizing RAM reads on the shared CPU path

- The fresh CPU-fallback profile after interrupt polling and opcode dispatch still sampled `MachineBus::read_byte` heavily (1,286 ms inclusive in a five-second profile; profile values are not additive and should not be compared as elapsed time).
- Added an early return for addresses below `ROM_BASE`. Since I/O (`0x8780..=0x87ff`) is above the ROM base (`0x8000`), this safely returns RAM without first checking I/O or ROM-bank decoding. Removed the now-redundant outer ROM threshold and unreachable RAM fallback.
- Three-run CPU-fallback WASM benchmark at x100, same headless Chrome 158 and 1280x720 setup: BASIC prompt/cursor median budget use decreased from 250.98% to 240.39% (4.2%); vector-loop median decreased from 313.88% to 296.06% (5.7%). Before/after three-run ranges did not overlap. These are host-specific `emulator.advance()` measurements, not total browser CPU use or release thresholds.
- Validation: 146 library tests passed (1 ignored), 32 CPU integration tests, 7 emulation tests, 25 machine tests; formatting and strict library Clippy passed; WASM build/binding generation passed; focused Rust Code Quality review found no issues. The browser profile passed on CPU fallback; WebGPU was unavailable. `wasm-bindgen-cli` was installed under workspace `target/tooling/` only, and all generated build/profile outputs are Git-ignored.
- Next: use profile/access-frequency evidence to evaluate remaining CPU work; do not make further bus/decoder changes without a matched benchmark demonstrating value.

## Comparing optimized and development WASM builds

- Built the same current WASM source in Cargo's optimized release profile and regenerated bindings, then ran the existing CPU-fallback browser benchmark three times in headless Chrome 158 at 1280x720. All three benchmark runs passed.
- Release x100 median `emulator.advance()` budget use was 40.88% for BASIC prompt/cursor (range 40.77–41.00%) and 44.60% for the vector loop (44.28–45.82%). The matched development-profile medians were 240.39% and 296.06%; the release build reduced measured call cost by about 83% and 85%, respectively. These are host-specific measurements against the 10 ms emulation budget, not total browser CPU use or universal thresholds.
- Added an optional release-profile build recipe to `docs/wasm.typ`; retained the development build as the default developer command. No emulator source changes were made in this comparison.
- Generated release bindings, benchmark reports, and build artifacts are Git-ignored under `target/`; restore the documented development-profile package before handing the workspace back.
- Next: use the release artifact for realistic performance checks and require release-profile hotspot evidence before adding further CPU/bus complexity.

## Profiling release WASM and testing decoder routing

- Captured a five-second release-profile CPU-fallback x100 profile. Exclusive sampled time was 443.6 ms in `Cpu6800::step` and 261.1 ms in `MachineBus::read_byte`; `Machine::step` accounted for 158.5 ms. The profile also spent about 3.5 seconds idle, and these sampled values are diagnostic self samples, not elapsed-time comparisons.
- Tested routing opcodes in groups 8-F directly to the word decoder for low nibbles C-F and to the memory decoder otherwise, avoiding a possible unrelated decoder probe. The focused CPU suite passed (32 tests), formatting passed, and the Rust Code Quality Reviewer found no actionable issue.
- Three release benchmark runs measured x100 medians of 40.68% BASIC prompt/cursor and 44.33% vector loop, versus the previous release baselines of 40.88% and 44.60%. The differences (0.20 and 0.27 percentage points) are negligible against overlapping run ranges, so reverted the trial rather than retain complexity without demonstrated value.
- Restored the documented development-profile WASM bindings. Release and development builds, raw profiles, and benchmark outputs remain Git-ignored under `target/`.
- Next: keep the current CPU implementation; only pursue another change if an isolated release-profile measurement predicts and then demonstrates a repeatable improvement.

## Evaluating thin-LTO WASM builds

- Built the unchanged emulator with `CARGO_PROFILE_RELEASE_LTO=thin` and `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1`, then ran the same three-run CPU-fallback browser benchmark in headless Chrome 158 at 1280x720. All three runs passed.
- Thin-LTO x100 medians were 38.47% of the 10 ms `emulator.advance()` budget for BASIC prompt/cursor and 42.40% for the vector loop, versus standard-release medians of 40.88% and 44.60%. The ranges did not overlap: BASIC 36.61–38.64% vs 40.77–41.00%; vector 41.48–42.85% vs 44.28–45.82%.
- The generated binding WASM was 1,407,620 bytes with thin LTO versus 1,474,987 bytes for standard release (about 4.6% smaller). These are host-specific measurements; no global Cargo profile settings were changed.
- Added an opt-in thin-LTO build recipe to `docs/wasm.typ`. The standard release build remains available, and development-profile bindings will be restored before handoff.
- Next: use the opt-in recipe when runtime performance matters; keep project-wide profile defaults unchanged unless cross-host validation justifies adopting them.

## Running release WASM with WebGPU

- Built the WASM library with `--release`, `CARGO_PROFILE_RELEASE_LTO=thin`, and `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1`, then generated web bindings with the locked-version `wasm-bindgen` CLI.
- Opened the local browser shell with a fresh query string after the build. Its Display status reports **WebGPU**, and the BASIC prompt is detected.
- The latest browser snapshot shows the emulator running with the Pause control and x20 selected. The initial Start control changed during the interaction; the attempted Start locator timed out, so no further controls were changed.
- The server was already running at `http://127.0.0.1:8000`; the current browser page ID is `a95e0e4d-3927-456a-b7fc-99233f876098`. Release bindings and build artifacts are Git-ignored under `target/`.

## Updating WASM/WebGPU performance documentation

- Updated the package WASM guide, browser-only WebGPU optimization plan, WebGPU renderer plan, WASM browser parity plan, design index, and render-pipeline audit. The plans now distinguish the separately authorized Rust optimization work from browser-only grades and keep visual-quality, active-renderer capture, device-loss, and cross-browser acceptance gates open.
- Added `dev/docs/tek4051/web/design/wasm-performance-optimization-report.typ`. It records the measured CPU-fallback and release/LTO results, WebGPU transient-geometry and cursor-dedup evidence, complexity introduced by retained changes, and rejected/deferred pooling, cursor-throttle, drawing-suppression, dispatch, and global-LTO changes. It also documents measurement boundaries and the mismatch between CPU-fallback x100 benchmarks and the live WebGPU x20 check.
- Updated the handoff with the current WebGPU runtime, accepted/rejected decisions, report location, and next useful performance evidence.
- Typst 0.15.1 compiled the new report and all six updated Typst documents, including `docs/wasm.typ`, to temporary PDFs successfully. `git diff --check` passed. `git check-ignore` reported none of the checked documentation files as ignored; temporary PDFs were kept outside the repository.
- Next: obtain matched headed x100 release measurements with WebGPU available when practical, then complete the open visual-quality, active-renderer capture, device-loss, and browser/device acceptance gates.

## Running release WASM with WebGPU again

- The previous browser page ID was no longer available, so reopened the same release shell URL using the existing thin-LTO WASM artifact and generated bindings. Started the emulator; the browser shows WebGPU selected, BASIC prompt detected, POWER on, and the Pause control. It is running at x1.
- Attempted a fresh thin-LTO build with `cargo build --locked`; Cargo 1.99.1 reported that it needed to update `Cargo.lock`. Did not allow dependency resolution or retain a lockfile change; restored the incidental lockfile diff and verified `git diff -- Cargo.lock` is empty.
- Confirmed `git diff --check` passes and the generated release WASM artifacts are ignored under `target/`. No source or dependency changes were made.
- If a fresh rebuild is required, resolve why the current Cargo version wants a lockfile update before removing `--locked`; the running browser currently uses the existing release artifact.

## Auditing the Rust package structure

- Performed a read-only boundary audit of `dev/src/4051-Emulator/rs/4051-Emulator-rs`; no production source or build configuration changed. The package is a Rust library (`rlib`/`cdylib`) with a thin native eframe binary and a feature-gated WASM adapter; native and browser targets reuse `Machine` and `EmulationState`.
- Counted 65 Rust files and 31,225 Rust lines including tests and the benchmark; 26 files are test-specific. Directory counts include nested tests/support: emulation 13, machine 11, tests 10, gui 8, flash_drive 7, native 6, host 4, platforms 3, package root 2, benches 1.
- Positive boundaries: machine display operations are platform-neutral; rendering/runtime ownership is in emulation; native worker/UI and browser/WebGPU lifecycle are adapters; public integration tests live under `tests/`, with private tests in separate files.
- Improvement candidates: `machine::gpib` selects `host_clock::local_datetime` by default despite an injectable RTC callback; `platforms/wasm/mod.rs` combines key translation, wasm-bindgen wrappers, Flash Drive archives, session commands, and snapshots in 1,268 lines; path-based native `FileEffect`s and byte-oriented WASM storage methods create two orchestration paths; root API aliases and path-included `host` modules need an explicit public-surface policy; package `readme.adoc` is empty. The `wasm` feature implies `webgpu`, preventing a compile-time CPU-only WASM variant.
- The native app is also included through a test-only path from `emulation/session.rs`; this is test-build coupling, not a production dependency. Treat file-size-based splits as candidates only; preserve the shared machine/emulation core and existing public aliases unless compatibility is checked.
- Validation was static source inspection and `find`/`wc` inventory; no tests/build were run because no code changed. Next: choose a specific boundary and compatibility objective before implementation.

## Splitting the tip commit by path

- Edited commit `3167042666b7f1fff71d16ea98084cb1550f7f98` during an interactive rebase.
- Created `src` with the original `dev/src/` change and `docs` with the remaining original changes; recorded this chat in the handoff and session history.
- The worktree was clean before the rebase. Because the branch tip is rewritten, publishing it will require coordination and a lease-protected force update if appropriate.

## Splitting the emulator and FlashDriveTests changes

- Edited commit `83a35af701ffec2aa46a4f25f863a9e728ca258d` during an interactive rebase.
- Created `tek4051` with all 60 changed files in `dev/src/4051-Emulator/rs/4051-Emulator-rs/`; the six remaining original changes in `FlashDriveTests-rs` go into `fdt`.
- Updated the handoff with the current split and branch status. The path partition is correct. `git diff --check` reports a trailing blank line at EOF in the original `performance.spec.mjs` change (line 452); it was left unchanged because this task only reorganized commits.

## Splitting the tip commit by `dev/src`

- Edited commit `6b3e9a973399a04185dc3a8e400359482bfac85b` during an interactive rebase.
- Created `src` with its six original `dev/src/` changes; the other original changes and this chat's handoff/history updates go in `co`.
- A later `dab39b9 Added readme.` commit appeared during the first attempt. After user direction, rebased the target and replayed that commit after the split, preserving its separate change.
- `git diff --check` reports a trailing blank line at EOF in the replayed `readme.adoc` (line 19); it was left unchanged to preserve the original content. No changed paths are Git-ignored.

## Splitting the dev-docs commit by path

- Edited commit `fca546bd4f0e976aa6e49e7f63e137b2e3bab336` during an interactive rebase.
- Created `src` with its two original `dev/src/` changes, `co` with its two original `dev/copilot/` changes, and `docs` with the 16 remaining original files under `dev/docs/`.
- Replayed the five later commits. Preserved the unrelated `Cargo.lock` change unstaged while rebasing, then restored it unstaged after completion.

## Rewording the dev-docs commit

- Reworded commit `7342c126249e8995c342f495ca94069f7f4ecd4e` from `docs` to `Added dev-docs.` during an interactive rebase.
- Verified the rewritten commit has the same tree as the original and replayed the two later commits. The worktree was clean before these handoff/history updates.

## Adding the Rust emulator performance watcher

- Added `.github/agents/rust-emulator-performance-watcher.agent.md` with read/search/execute access and a read-only review scope for performance-sensitive emulator paths. Added a workspace instruction to invoke it for relevant Rust changes, complementing rather than replacing correctness and test reviews.
- The watcher uses matched existing native/WASM benchmarks, separates WebGPU from CPU fallback, requires multiple runs where practical, and adopts no universal threshold. If a safe comparable baseline is unavailable, the result must be inconclusive; it must not alter source, dependencies, lockfiles, or the user's worktree.
- Validation: `git diff --check` passed. `git status --short` showed only the new agent and updated Copilot instructions before the handoff/history updates; `git check-ignore` found neither ignored. No performance benchmark was run because this task changed only Copilot configuration.
- Next: use the watcher on a future performance-sensitive Rust change and capture a matched baseline before editing.

## Expanding the performance watcher release matrix

- Updated the watcher to explicitly survey release builds and runtime behavior for native Rust egui and Rust WASM, each with CPU fallback and WebGPU. It reports all four entries independently and requires runtime confirmation of the selected renderer.
- Documented that the native Battlestar benchmark covers only shared emulation execution, not the eframe renderer. Where native UI renderer measurements or any other comparable result are unavailable, the corresponding matrix entry is inconclusive; build success alone is not a performance pass.
- Updated the workspace invocation rule and handoff accordingly. Validation: `git diff --check` passed; `git check-ignore` found none of the task changes ignored. No builds or benchmarks were run because only Copilot configuration and handoff/history changed.
- Next: apply the four-entry release matrix to the next performance-sensitive Rust emulator change.

## Updating the Copilot setup guides

- Updated `dev/copilot/setup/setup.adoc` with reusable guidance for an optional performance reviewer, matched release comparisons, and explicit inconclusive results when evidence is unavailable or not comparable.
- Updated `dev/copilot/setup/setup-rust.adoc` with reusable Cargo, egui, Rust test, and validation guidance only; moved Tektronix-specific rules out.
- Created `dev/copilot/setup/setup-repository.adoc` for fork constraints, actual package/test paths, local review and handoff workflows, macOS packaging, and the native egui/WASM × CPU-fallback/WebGPU release matrix. It records renderer verification, matched-measurement conditions, and the limit that the Battlestar core benchmark does not measure renderer performance.
- Updated the handoff. No emulator code changed; no builds or benchmarks were run. Next: use the documented workflow on the next performance-sensitive runtime change.

## Investigating binary-program launch from the Flash Drive view

- Traced native directory selection and browser Flash Drive launch through the shared session. `EmulationState::launch_flash_program` accepts only files with `ASCII` type and `PROG` usage; all other selections receive the “not an ASCII PROG” notice. Native UI tests explicitly assert `DATA` and `BINARY PROG` rejection. The browser button may be enabled for a selected binary file, but the shared launcher returns the same rejection.
- Confirmed this is a launch-integration limitation, not evidence of broken binary import/storage: tape `BOLD` supports selected `BINARY PROG`, and a ROM-level test verifies `CALL "BOLD",5`, reload, and execution. ASCII UI launching types `FIND @5:n`, `OLD @5:`, `RUN`; binary UI launching would need `FIND @5:n`, `CALL "BOLD",5`, `RUN`.
- No code changes or runtime tests were made. Next: decide whether the directory launch action should support `BINARY PROG` through the R05 BOLD path.

## Starting release WASM with WebGPU

- Reopened the existing thin-LTO release shell at `http://127.0.0.1:8000/dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser/?release-build=thin-lto-20261119-0826`. The browser loaded `/target/wasm/pkg/tek4051_emulator_bg.wasm`; status reported WebGPU, BASIC prompt detected, POWER on, and the machine running at x1.
- Reused the existing release artifact rather than rebuilding: the earlier locked build was blocked when Cargo requested a lockfile update. No build or source changes were made for this launch.

## Fixing binary Flash Drive launch sequencing

- The user reported `CALL "BOLD",5` echoing as `call02bold2.5` with a syntax error, with `RUN` arriving before BOLD returned. The live screen confirmed both defects: raw text bytes did not map quotes/commas to Tek keys, and fixed per-key press/release timing did not wait for BASIC to finish a command.
- Updated the shared program loader to map printable text through the existing Tek keyboard mapper, send carriage return as Enter, and stage `FIND`, `OLD`/`CALL "BOLD",5`, and `RUN` as separate commands. After each line, the shared session resumes only when it detects BASIC readiness; no fixed delay is used for command completion. Added focused sequencing/key-mapping assertions and changed the native runtime test to advance in frame-sized steps so readiness callbacks are observed.
- Updated the browser launch rejection assertion and the WASM API comment/docs for ASCII and binary programs. The WASM guide notes that successful command delivery does not certify target-model/ROM compatibility.
- Validation: `cargo test --locked -p tek4051-emulator` passed (311 passed, 1 ignored); the program-load focused tests passed (2/2); the Flash Drive filter passed (41 unit tests and 22 GPIB tests); the targeted Playwright FlyBy launch test passed (1/1); Rust formatting, JS syntax, library-only strict Clippy, release WASM build/bind, and Typst compilation passed. All-target strict Clippy still reports unrelated unused imports in `native/backend/tests.rs`.
- The release browser initially reported WebGPU with no error after clearing its cache. At x20 the FlyBy screen showed `FIND@5:2`, `CALL "BOLD",5`, and `RUN` on separate lines, then `SYSTEM ERROR - MESSAGE NUMBER 0`; the cause is unverified and could involve a 4054/Option 30 requirement. During the run the renderer fell back to CPU because synchronous browser screen-copy readback is unsupported. Generated WASM/bindings under `target/` are Git-ignored.
- Next: investigate program/model compatibility or browser readback only if requested; these are separate from the corrected command sequencing.

## Follow-up review and launch-capacity accounting

- Code review confirmed that the current text mapper emits Tek key/modifier pairs for scripted characters, including Shift+2 for quotes. It also found that native `Command::Launch` reserved for the shorter ASCII script even though a binary launch queues a longer BOLD call. Updated the reservation to use the binary sequence's size conservatively and adjusted the exact-capacity regression.
- Test review found the directory integration test only confirmed the launch was accepted. Extended the binary fixture test to advance through FIND and assert that the second line starts with `CALL` rather than ASCII `OLD`; the first placement was mistakenly in the ASCII test and failed, then was moved to the binary test, which passed.
- `cargo fmt --all -- --check` and the full package tests passed after correction (311 passed, 1 ignored); strict library Clippy passed. The native backend test module still emits pre-existing unused-import warnings.
- The Performance Watcher reports all four native/WASM release build configurations passed, but the performance result is inconclusive: its benchmark lacks a matched baseline and does not measure renderer performance. Renderer-specific candidate/runtime evidence remains incomplete.

## Fixing the Pi benchmark RTC failure in WASM

- Reproduced `unreachable` while the Pi program read its clock on the release WebGPU page. The RTC callback reached `host_clock::local_datetime`, which called `std::time::SystemTime::now()`; that API is unsupported on `wasm32-unknown-unknown`. The recursive wasm-bindgen aliasing error followed the initial trap when the page reused the poisoned emulator object.
- Added a browser-only `js_sys::Date::now()` timestamp conversion with non-finite, integer-range, and Jiff-range errors preserved. Native sampling still uses fallible `Timestamp::try_from(SystemTime::now())`; added the target-specific `js-sys` dependency.
- Reworked the Playwright regression to load and run the supplied Pi BASIC program, wait for the calculation to return to a fresh BASIC prompt, observe RTC I/O on/off transitions, and assert there is no runtime error.
- Validation passed: 149 library tests (1 ignored), strict native and WASM library Clippy, release WASM build and wasm-bindgen, focused Pi Playwright test (1/1), Rust formatting, JavaScript syntax, and `git diff --check`. Code Quality review found no remaining issue. The focused live run on WebGPU used input `1`; the display showed the Pi result and calculation time, BASIC prompt returned, I/O was off, and no Rust-level error appeared.
- Generated WASM, bindings, Playwright output, and local tooling are under Git-ignored `target/`. The FlyBy program's separate `SYSTEM ERROR - MESSAGE NUMBER 0` remains unclassified.

## Investigating the Games bulk-import stall

- Measured and selected all 56 regular files from `dev/src/4051-Emulator/web/FlashDrive/Games` in the release WASM browser shell. Imports completed both with the emulator stopped and while running at x100 on WebGPU; the x100 import took about one second and produced the “56 Flash Drive file(s) imported” notice, 56 stored rows, enabled controls, and no page/runtime error.
- The completion notice is below the scrollable file table. The worker request path already rejects worker `error` and `messageerror` events. No source or test changes were made because the reported stall was not reproduced.
- The x100 release/WebGPU test page remains open at `?bulk-import-x100=20261119-2223` (page ID `22eb9d1b-ef82-4033-b1ab-63c63ef373a0`). If the issue persists, capture whether “Processing 56 selected file(s)” remains with controls disabled versus the files being stored without a visible notice, and provide browser, renderer, and CPU-speed details.
