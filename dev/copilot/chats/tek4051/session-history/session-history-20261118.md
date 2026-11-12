# Session history — 2026-11-18

## Investigating x100 keyboard latency

- Read the current handoff and traced the native worker and shared runtime timing path. The worker drains up to 64 queued commands before advancing emulation; at x100, each 10 ms runtime interval runs 16,352 CPU instructions (2,264 at x1).
- `advance_realtime` caps each worker iteration to one 10 ms logical tick and records excess elapsed wall time as coalesced time. A process sample of the running native WebGPU emulator (PID 29782) reported 108.3% CPU after 4:53, consistent with one-core saturation. This suggests CPU work may delay both queue draining and timed input progression, but exact queue wait and key-to-PIA latency remain unmeasured.
- No emulator timing, queue, or machine behavior was changed. The native process was left running; the pre-existing untracked `dev/docs/tek4051/keyboad-input-2.adoc` was untouched.
- Next: instrument x1/x100 worker tick duration, command dequeue delay, and PIA press/release timing to isolate transport delay from sequencer/emulation pacing.

## Investigating input backlog and cursor persistence

- The user reports that rapid typing at x1 can leave multiple seconds of input backlog and that x100 sometimes shows both current and previous cursor marks.
- Traced manual input pacing: each key tap has a 100 ms minimum press plus 100 ms minimum release, a nominal maximum of five taps per second. The host worker's 1,024-transition FIFO is decremented when commands are taken; transitions then move into a separate unbounded `InputSequence` deque, so the existing capacity does not bound total pending input.
- Confirmed the shared runtime keyboard interrupt cadence is 10 ms. No timing change was made pending confirmation of the desired minimum phase duration and queue-overflow behavior.
- Cursor marks are transient in the same sense for both Rust and the legacy web reference: they remain until a 100 ms display refresh removes cursor-intensity pixels. Multiple cursor positions within that interval can therefore remain visible together, particularly at high speed. No renderer behavior was changed pending confirmation whether strict single-cursor display should replace the reference phosphor behavior.
- No emulator source changes were made. Next: ask the user to confirm the input pacing/queue policy and desired cursor persistence.

## Bounding sequenced keyboard input

- Continued the user-approved input-backlog work. A 10 ms press/release trial caused real-ROM input regressions, so timing was kept at 50 ms press / 30 ms release and unified across manual, panel, and scripted keys.
- Extended the native 1,024-transition limit to account for host-queued, in-flight, sequencer-pending, Auto Load, and directory-launch transitions. Added a regression proving that generated scripts cannot bypass the aggregate limit.
- Advanced the worker clock at Run/Input/script/modal and file-dialog boundaries so elapsed time before a key starts is not credited to its press phase, and modal time does not consume that phase. The ROM integration test exercises `PRINT 1` at an 80 ms/tap cadence and checks final output; transient cursor-readiness is not treated as an input gate. No cursor rendering behavior changed.
- Validation: `cargo fmt --all -- --check`, `git diff --check`, and `cargo clippy --locked -p tek4051-emulator --all-targets --all-features -- -D warnings` passed. Default focused tests passed (137 library tests, 1 ignored; 7 emulation; 6 input), and WebGPU-enabled focused tests passed (140 library tests, 3 ignored; 7 emulation; 6 input). `cargo check --locked -p tek4051-emulator --no-default-features --features wasm --target wasm32-unknown-unknown --lib` passed. Both Rust reviewers found no remaining actionable issues. Cargo emitted the existing ZipWriter `WriteZero` diagnostic and `block v0.1.6` future-incompatibility notice.
- Deterministic worker tests cover modal suspension during both the active press phase and the release gap before a queued next key. Manual x1/x100 input-speed validation remains; no native emulator process was restarted. Cargo output under Git-ignored `target/` was generated; source/test changes are not ignored.
- The user requested deletion of `dev/docs/tek4051/keyboad-input-2.adoc`; the specific untracked file was deleted, and the current handoff now records that its keyboard-path note was removed.

## Native Flash Drive file selection report

- The user reports that selecting all files in a directory does not work in the native Flash Drive selection dialog, while the WASM picker succeeds.
- Inspection found native import calls `rfd::FileDialog::pick_files()` without an extension filter, and the path importer processes every selected path; an existing native regression covers multiple loose and ZIP files. The installed macOS `rfd` 0.17.2 backend enables multiple selection.
- Clarification: Command-A does not select all in the native picker, but selecting the first item and Shift-clicking the last selects the range; after confirming, all files appear in the Flash Drive directory. This confirms multi-file import works. The picker delegates to the OS through `rfd`, so adding an explicit select-all shortcut would require replacing it with an in-app picker; no such UI change was made.

## Native folder-based Flash Drive import

- The user changed the requested workflow to select a folder and list/import all files in it through the Flash Drive directory.
- Replaced the native multi-file chooser with a folder chooser. The app now processes every immediate regular file in sorted path order, includes ZIP archives, does not recurse into subfolders, and reports empty/unreadable folders explicitly; normal per-file validation and partial-success notices remain in effect.
- Added regressions for loose files plus ZIP contents, deterministic ordering, nested-folder exclusion, per-entry partial success, empty/missing-folder errors, and the UI import/result path. Default library/native tests passed (141 passed, 1 ignored); WebGPU tests passed (144 passed, 3 ignored). Strict Clippy, formatting, and `typst compile dev/docs/tek4051/basic.typ target/basic-guide.pdf` passed.
- Rust Code Quality review found no remaining production issue after correcting the UI regression; Rust Test Quality review found no remaining gaps. The OS folder picker itself remains platform-controlled and is not invoked by deterministic tests.

## Running the native WebGPU emulator

- At the user's request, checked for an existing emulator process (none was running) and launched `cargo run --locked -p tek4051-emulator --features webgpu`.
- Cargo built successfully and reported `Running target/debug/tek4051-emulator`; the process was verified active as PID 68862. No WebGPU fallback warning appeared in captured output, but the window and active-renderer status were not visually inspected.
- Build output under Git-ignored `target/` was generated.
- `git diff --check` on the changed paths passed. The full check still reports a pre-existing trailing blank line in `dev/docs/tek4051/keyboad-input.adoc:272`; that unrelated worktree change was preserved. The generated `target/` output is Git-ignored.

## Profiling and reducing x100 input latency

- Sampled the still-running WebGPU emulator without stopping it. A one-second macOS sample recorded 197 emulation-worker stacks; 161 were under `HostRuntime::sync_display` / `WebGpuVectorRenderer::finish_display_batch`, including 81 in `vertex_bytes`. Process CPU was 120.8% at a subsequent check, indicating the display path is consuming more than one core-equivalent.
- Replaced CPU-expanded six-vertex-per-segment geometry with one compact per-segment GPU instance; the vertex shader generates the same antialiased quad from CPU-validated segment data and normalized direction. The vertex buffer payload falls from 192 to 32 bytes per segment, while segment validation and ordered operation handling are preserved.
- Validation passed: WebGPU library/emulation suites (145/7 passed, 3 ignored in the library suite), both explicit native adapter-backed public renderer tests and the private screen-copy test, WASM WebGPU compile check, `cargo fmt --all -- --check`, strict all-target/all-feature Clippy, and `git diff --check`. Tests emitted the existing ZipWriter diagnostic; the WASM check retains its existing `MAX_PENDING_SCREEN_COPIES` dead-code warning.
- Rust Code Quality review found no actionable issue. Test Quality review called out missing normalized/zero-length direction assertions and public invalid-batch index/state-preservation coverage; added these and verified them with the focused and adapter-backed tests. Follow-up review confirmed no actionable test gaps remain.
- PID 68862 was not restarted by this work but was absent at the final process check; no other emulator process was found, and the exit cause is unknown. The rebuilt executable remains unlaunched, so post-change x100 behavior is unverified. Cargo build/test outputs under Git-ignored `target/` were generated. Next: ask the user whether to launch the rebuilt app, then test x100 typing and re-profile.

## Launching the optimized WebGPU build

- The user authorized launching the rebuilt emulator. `cargo run --locked -p tek4051-emulator --features webgpu` succeeded without a fallback warning; PID 77467 was verified running. The earlier PID 68862 was already absent, and no command in this turn stopped it.
- Computer Use listed the emulator window but could not bind to it because `tek4051-emulator` is not installed for that service. No UI action or x100 typing test was performed; do not bypass the UI-binding limitation. The user can test x100 input in the running process, or UI-based verification can resume if binding becomes available.

## Reviewing the unofficial keyboard PIA handshake note

- The user reported that the optimized build made x100 typing noticeably faster, and x1 faster too, but x100 remains unacceptable. A later profiling attempt found that PID 77467 was no longer running, so no post-change sample was captured.
- Checked the selected note's address and control-bit claims against the implementation. `$878C` maps to the DisplayY PIA; the keyboard PIA base is `$87A8`. Reading a PIA data register clears that PIA's control flags only if CRA bit 2 selects data. CRA bit 3 selects CA2 input-edge polarity; the current model has no automatic CA2 output handshake on Port A read.
- Conclusion: the non-overwrite principle could help if a verified keyboard-PIA read event is used, but the note's address/CA2 description cannot safely serve as acknowledgment as written, and a PIA read would not prove BASIC accepted the character. Prior 10 ms pacing experiments regressed real-ROM input, so no timing/code change was made.
- Next: if further pacing work is requested, trace the ROM's reads at `$87A8` and actual CA2 transitions before altering the sequencer. Do not rely on the suggested 2–5 ms delay without evidence.

## Reviewing emulator performance architecture

- Clarified that the 100 ms interval is display phosphor/cursor refresh, while CPU/keyboard callbacks and the native worker tick are 10 ms. The shared input sequencer holds each tap for 50 ms and releases for 30 ms, a theoretical 12.5 taps/s before scheduler/render delays; the observed roughly 10/s is near that configured ceiling.
- The runtime budget formula yields about 2,264 instructions per 10 ms at x1 and 226,359 at x100. The worker caps each real-time advance to one 10 ms tick and coalesces excess elapsed time, so overruns reduce both delivered simulation speed and timed input progress. Reducing the 100 ms phosphor interval would not reduce key latency.
- The prior profile found GPU display composition dominated the worker; instanced geometry improved both x1 and x100 by user report, but x100 remains unacceptable. No post-change profile was possible because the emulator process was no longer present.
- Next performance checks: compare the current debug-style launch with a release build; profile CPU execution, renderer composition, duplicate CPU-reference work, and worker tick duration at x1/x100. Consider rendering at presentation cadence separately from machine execution only after benchmarking and preserving screen-copy/cursor semantics. No code change was made in this review.

## Adding a native workload graph and speed probes

- Added a bounded 300-sample series measuring `EmulationState::advance_realtime` duration as a percentage of the 10 ms worker budget. The native UI renders the recent samples, a 100% over-budget threshold, average/peak values, and a direct speed selector; applying a speed command clears the prior graph history.
- Extended the ignored native worker probes to sample each supported speed for one second and print average, p95, peak, and percentage of over-budget advances, alongside coalesced wall time. Debug/WebGPU showed intermittent overruns at x5, 27% over-budget advances at x10, clear saturation at x20 (166.7% average, 90% over budget), and severe overload at x100 (824.2% average). Release/WebGPU stayed below the 10 ms budget through x100 (42.0% average at x100). These are advancement-path measurements, not end-to-end BASIC printing rates.
- Commands used: `cargo test --locked -p tek4051-emulator --features webgpu --lib reports_native_webgpu_worker_catch_up_by_speed -- --ignored --nocapture` and the same with `--release`. Full WebGPU library tests passed (149 passed, 3 ignored); strict all-target/all-feature Clippy, formatting and whitespace checks passed.
- Rust Code Quality review found no issues. Test Quality review identified that the initial graph test checked labels but not the plotted line; it now verifies the emitted series path is ordered oldest-to-latest and rises from 50% to 125%, then the full suite passed again.
- No GUI app was launched. Build/test outputs under Git-ignored `target/` were generated. Next: use the graph while comparing real BASIC printing at x10/x20/x100 in Debug and Release/WebGPU; investigate remaining rendering/worker overhead only from those observations.

## Launching the WebGPU Debug emulator

- At the user's request, started `cargo run --locked -p tek4051-emulator --features webgpu`. The default Cargo profile is `dev`/Debug; compilation succeeded and PID 70075 was verified running. No fallback error was captured, but the window and active renderer were not visually inspected.
- The detached shell later completed with exit code 0; a subsequent process check found PID 70075 absent. The exit cause is unknown, and the emulator was not restarted.
- Build output under Git-ignored `target/` was generated.

## Adding unlabeled one-second workload ticks

- Completed the timestamped workload-sample migration through the native backend, graph, benchmark summaries, and tests. The graph now spaces samples by actual worker-elapsed time and draws unlabeled tick marks at integer one-second boundaries along its bottom edge; removed the former “older” and “latest” text.
- Strengthened the GUI regression with samples at 0.25–3.25 seconds, including an exact 100% sample, checks for every plotted timestamp/value, the threshold line's position and span, all three one-second tick positions, and no text below the graph baseline.
- Validation passed: `cargo test --locked -p tek4051-emulator --features webgpu --lib` (149 passed, 3 ignored), strict all-target/all-feature Clippy, `cargo fmt --all -- --check`, and scoped `git diff --check`. Code and test reviewers found no remaining actionable issues. The existing `block v0.1.6` future-incompatibility notice and test ZIP-writer `WriteZero` diagnostic remain.
- Cargo artifacts are under Git-ignored `target/`; no emulator process was launched. Next: use the graph for actual Debug/Release BASIC-print comparisons when native UI testing is available.

## Running the Release WebGPU emulator

- The user requested the Rust emulator with the WebGPU renderer in Release mode. No existing emulator process was running, so launched `cargo run --locked --release -p tek4051-emulator --features webgpu`.
- Cargo built the optimized `release` profile and started `target/release/tek4051-emulator`; PID 76864 was verified active. No fallback warning appeared in captured output; the window and active renderer were not visually inspected. The process is left running.
- Release build artifacts are under Git-ignored `target/`.

## Improving workload graph readability and motion

- Made the graph's unlabeled one-second x-axis marks 8 px long with 1.5 px strokes, matching the plotted line weight.
- Reduced the perceived animation judder by moving the time viewport continuously at frame time between worker samples, extending the latest workload value as a flat segment for at most 50 ms. The graph stops advancing if samples stall and splits the plotted line across gaps longer than the smoothing interval.
- Expanded GUI regressions to check tick geometry and positions across frames 5 ms apart, before sample receipt, and after the 50 ms cap. Rendered samples 50 ms apart remain connected, 51 ms gaps split into separate paths, and an isolated sample still appears at its measured position.
- Validation passed: focused graph tests and full WebGPU-enabled library suite (150 passed, 3 ignored), strict all-target/all-feature Clippy, formatting, scoped whitespace checks, and optimized WebGPU Release build. Both Rust reviewers found no remaining actionable issues. The existing future-incompatibility notice and test ZIP-writer diagnostic remain.
- Restarted the assistant-launched Release executable with the updated build; PID 83411 was verified active. No fallback warning was printed, but the window/renderer was not visually inspected. Build artifacts remain in Git-ignored `target/`.

## Investigating drawing-correlated workload spikes

- The user reports that drawing-free BASIC execution has low workload, vector drawing causes transient spikes, and the blinking cursor causes distinct spike groups about four times per second at x1.
- Source tracing confirms the workload metric covers `EmulationState::advance_realtime`, including CPU batch execution and display synchronization. With WebGPU enabled, each display operation is first applied to the CPU reference renderer; collected events are also translated into GPU geometry. A dirty GPU batch copies/composes the display texture and submits the command buffer. Each transient cursor mark becomes two GPU point segments.
- These facts support a display-path correlation but do not identify the exact expensive substage. Earlier profiling sampled WebGPU composition/instance-preparation work as prominent; no new layer-level timers or cursor-cadence trace were captured. Next: separately time CPU instructions, CPU-reference rasterization, GPU event/instance preparation, and batch submission/composition for drawing-free, vector-heavy, and cursor-only cases. Preserve renderer behavior until those measurements identify the bottleneck.

## Assessing automatic display benchmarks

- The user asked whether this can be tested automatically or requires an interactive session. Confirmed the ignored WebGPU speed probe is already headless but measures aggregate default-ROM work rather than isolating drawing.
- A controlled headless comparison can use matched CPU instruction workloads with no display operations, vector output, and cursor marks through the same emulation/WebGPU path. Interactive validation would only be needed to reproduce a specific user BASIC program or assess visible UI behavior; that program is not yet attached to the benchmark.
- No benchmark was run in this turn. Next: implement/run an isolated headless comparison before drawing conclusions about the reported spikes.

## Running the headless x1 WebGPU drawing benchmark

- At the user's request, added and ran an ignored headless native-worker probe with four matched cases: RAM-only control versus vector PIA output, and RAM-only control versus cursor PIA reads. The cursor fixture issues one mark about every 25 x1 ticks (approximately four per second); the vector fixture continuously generates output. Pixel-buffer checks confirm each output case changes the display while its paired control does not.
- Release/WebGPU results over three seconds per case (10 ms worker-budget percentages; 245–251 samples): vector control average 0.9%, p95 5.3%, peak 9.6%; vector output 4.9%, 10.9%, 15.8%. Cursor control 0.9%, 5.2%, 9.8%; cursor output 1.3%, 9.6%, 12.8%. No case had over-budget ticks. Vector drawing has a clear added cost; sparse cursor output has a small average increase but higher tail/peak. These measurements do not identify per-stage costs or establish behavior for the user's exact BASIC program.
- Validation passed: the ignored Release/WebGPU benchmark, the WebGPU library suite (150 passed, 4 ignored), strict all-target/all-feature Clippy, `cargo fmt --all -- --check`, and scoped `git diff --check`. Cargo warned about the existing `block v0.1.6` future incompatibility and the library suite emitted the existing ZIP-writer diagnostic. Cargo outputs are under Git-ignored `target/`; no GUI process was present or started.

## Correcting benchmark window and cursor checks

- Updated the ignored native WebGPU probe so each measured interval waits for a worker sample at or beyond the baseline timestamp plus three seconds, and reports actual sample count and observed timestamp span. Added a semantic preflight that confirms the cursor fixture emits exactly four `TransientCursorMark` events over 75 x1 ticks; framebuffer checks remain separate.
- Rust Test Quality review found that an endpoint sample alone did not guarantee representative coverage. Added assertions for at least 95% of the requested timestamp span, at least 50 samples/second, and no inter-sample gap above 100 ms. Follow-up review confirmed that the sparse-window finding is resolved with no remaining actionable issues.
- The corrected Release/WebGPU probe passed: vector control 1.2% average / 8.2% p95 / 11.2% peak; vector output 6.5% / 14.5% / 27.0%; cursor control 1.4% / 9.7% / 13.4%; cursor output 1.7% / 10.1% / 29.3%. All four cases had zero over-budget ticks, with 247–253 samples covering 2.98–2.99 seconds and largest sample gaps of 14–15 ms.
- WebGPU library tests passed (150 passed, 4 ignored), strict all-target/all-feature Clippy passed, and formatting and whitespace checks passed. Cargo warned about existing `block v0.1.6`; the suite emitted its existing ZIP-writer diagnostic. Cargo output is under ignored `target/`.

## Launching the native Release/WebGPU emulator

- Launched `cargo run --locked --release -p tek4051-emulator --features webgpu`. The native GUI process started as PID 5840 and remained alive 10 seconds later; the window was not visually inspected. No source changes were made for this launch.

## Investigating idle x100 workload

- The user reported roughly 170–175% average and 330% peak workload at x100 in the running Release/WebGPU app. Two five-second macOS samples reproduced the worker wait: first 2,638/3,636 samples in `finish_display_batch` and 2,622 in its queue-submit lock wait; second 2,592/3,614 in the batch flush and 2,564 in the same wait path. The second sample also captured 3,472 UI-thread samples in `Surface::get_current_texture`.
- Local WGPU 25.0.2 source shows `Surface::get_current_texture` holding `device.fence.read()` during Metal `next_drawable` acquisition, while `Queue::submit` requires `device.fence.write()`. The native emulator renderer shares eframe's device/queue, making surface acquisition a direct candidate for the worker's measured wait. `record_segments` appeared in both samples, consistent with dirty display batches; the precise event rate/source (possibly the transient cursor) remains unmeasured.
- The graph records wall-clock time in `advance_realtime` divided by its 10 ms budget, so 175% is 17.5 ms elapsed per tick, not CPU utilization. The earlier 42% x100 probe used an independent headless device/queue and is not directly comparable. User smoke tests remain acceptable at all supported speeds, so no user-facing regression is established.
- No emulator source changes were made; temporary profile reports were removed. Next: instrument display-event/submission cadence and fence-wait time in the live eframe path before considering optimization.

## Deferring native WebGPU submissions

- Implemented bounded worker-to-UI command-buffer submission in `emulation/webgpu.rs` and wired the native app to drain the queue after its UI update, before eframe acquires the surface. Ordinary queue submission no longer occurs inside the measured worker tick. `ScreenCopy` explicitly flushes the deferred work before its synchronous readback; direct submission remains the default and WASM keeps its prior path.
- Added native GPU regressions for deferred presentation, queue overflow with staged-batch retry, and screen-copy/erase ordering. All three adapter tests passed when explicitly run. The WebGPU library suite passed (150 passed, 6 ignored), strict all-target/all-feature Clippy passed, the optimized native Release check passed, the WebGPU WASM check passed with the existing `MAX_PENDING_SCREEN_COPIES` dead-code warning, and formatting/whitespace checks passed.
- Cargo emitted the existing `block v0.1.6` future-incompatibility notice; the test suite also emitted its ZIP-writer diagnostic. Cargo outputs are under Git-ignored `target/`. No GUI process was restarted, so post-fix idle x100 graph values and visual behavior remain unmeasured. Next: relaunch Release/WebGPU and compare the idle workload graph. The pre-existing staged `native/backend/tests.rs` change was left untouched.

## Running Release/WebGPU after the fix

- Confirmed no emulator process was running, then launched `cargo run --locked --release -p tek4051-emulator --features webgpu`. The optimized build completed with the existing `block v0.1.6` future-incompatibility notice.
- Verified `target/release/tek4051-emulator` remained alive as PID 36500 five seconds after launch. The window and workload graph were not visually inspected; next: verify the display and measure idle x100 workload in this post-fix process.
- The launch generated build output under Git-ignored `target/`.

## Confirming x100 interactive input

- The user reports that interactive input at CPU speed x100 worked great after the deferred WebGPU submission change, describing it as a massive improvement.
- This is qualitative confirmation of improved input responsiveness; no quantitative workload-graph measurement or agent-side visual inspection was captured. The Release/WebGPU process was PID 36500 at launch. Further graph measurement is optional unless needed to quantify the improvement.

## Running the WASM emulator

- Built the WASM library with `cargo build --locked --target wasm32-unknown-unknown -p tek4051-emulator --no-default-features --features wasm --lib` and generated web bindings with `target/wasm-tools/bin/wasm-bindgen` version 0.2.130, matching the locked crate.
- Started `python3 -m http.server 8000 --bind 127.0.0.1 --directory .` and verified HTTP 200 responses for the browser shell, generated JavaScript, and `.wasm` asset. Opened `http://127.0.0.1:8000/dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser/` in the browser and started the emulator at default x1; the page reports Running and BASIC prompt detected.
- Generated bindings and WASM build artifacts are under Git-ignored `target/`. The server and browser session remain running.

## Reviewing the proposed WASM frontend layout

- The user proposed a title bar and menu bar above a two-column layout: display plus Busy/Break/I/O/Power status bar on the left, Flash Drive directory on the right, and existing controls in a window opened by a Controls menu button.
- Advised that this is a good separation of the display and frequent work area from less-frequent controls. Recommended responsive stacking on narrow screens, independent Flash Drive scrolling, reusing existing controls/event handlers, and clarifying whether status-bar buttons are indicators or actionable controls before implementation. No frontend code was changed.

## Implementing the WASM frontend layout

- Implemented the title/menu bars, display/status column, collapsed-by-default Flash Drive panel on the right, Controls dialog, narrow-screen right-side overlay, and aspect-ratio-preserving canvas. BUSY/BREAK/I/O/POWER remain indicators driven by the browser snapshot; original control IDs and handlers are retained.
- Added layout coverage, updated Flash Drive tests to navigate the collapsed panel/dialog, and documented the new browser layout. Status-lamp DOM attributes now update only when their visible/accessibility state changes.
- Full Playwright suite: 22/24 passed; the two failures were archive responsiveness assertions at a 100 ms animation-frame gap. Repeating the maximum-size export check yielded 2 passes and 1 failure (100.1–116.8 ms on failures); the `<100 ms` threshold was not relaxed. Layout tests passed. These timing failures had also occurred before the layout change and are not evidence that the canvas CSS fix failed.
- Typst documentation compilation and `git diff --check` passed. Source changes are not ignored; generated Playwright reports and the compiled PDF are under Git-ignored `target/`. Next: the layout is implemented; investigate archive responsiveness separately if requested.

## Moving WASM controls into tabs

- Following the user's feedback that the Controls dialog did not work well, moved the existing Flash Drive and emulator/front-panel controls into Flash Drive and Controls tabs in the right-side Tools panel. The existing top Controls button now expands that panel and selects its Controls tab; Flash Drive remains the default tab, and the panel remains collapsed by default and a right-side overlay on narrow screens.
- Added tab keyboard navigation and browser regressions for tab selection, menu-button routing, controls behavior, narrow-screen placement, and canvas aspect ratio. Live inspection found the earlier canvas size could distort at desktop dimensions; the CSS now caps width against viewport height, and desktop/narrow ratio checks pass.
- Focused layout tests passed (2/2); the full Playwright suite passed 23/24. The lone failure is the existing maximum-size archive-export frame-gap assertion at 100.0 ms vs. the strict `<100 ms` threshold. Typst documentation compilation and `git diff --check` passed. Reloaded the shared WASM page and left it paused at x1 with the Controls tab visible.

## Moving core controls to the WASM menu bar

- Removed the menu-bar Controls button and the title-bar Paused/CPU xN labels. Moved the Start/Pause toggle, Reset, and labeled CPU speed selector to the menu bar in the requested order; Start/Pause reflects running state with `aria-pressed`. The Controls tab retains the other settings and front-panel keys.
- Updated browser tests and documentation. Focused layout tests passed (2/2), the full Playwright suite passed (24/24), Typst compilation and `git diff --check` passed. Reloaded the shared WASM page; it is paused at x1 with the Tools panel collapsed.

## Fixing click-to-type focus on the WASM display

- Reproduced the reported failure in a real-WASM browser test: the display pointer handler focused the hidden textarea on `pointerdown`, but the browser's click default action immediately blurred it. Changed focus handling to the surface `click` event, after the default action.
- Added a browser regression that starts BASIC, clicks the display, checks the keyboard textarea remains focused, types a command, and verifies the display echoes text. The focused test passed.
- Full Playwright suite: 24/25 passed; only the previously flaky maximum-size archive export frame-gap assertion failed at 100.0 ms against `<100 ms`. `git diff --check` passed. Did not reload the shared page, which is currently running, to avoid discarding its emulator state; refresh it to load the updated script.

## Verifying live WASM display input

- The shared browser kept executing a cached older `input.js` after an ordinary reload. A cache-bypassing reload loaded the current click handler.
- With the emulator running at x1 and BASIC ready, clicking the display left keyboard input focused. Typing `PRINT 123456789` echoed on the display, increasing lit canvas pixels from 0 to 812. The page remains running at x1; no product-code change was needed.
- The user confirmed that display focus now works.

## Resizing the WASM shell to the browser viewport

- Made the shell use the available viewport dimensions and constrained the display column so the 1046×830 renderer canvas can scale within its flex area without distortion or page scrolling.
- Added responsive layout coverage for 1280×720 and 800×600, updated the WASM layout documentation, and verified the shared browser page at 1280×720 with no document overflow. Layout tests passed (3/3), Typst compilation and `git diff --check` passed.
- The user noted the 1024×780 graphics area and 72×35 text capacity; project documentation confirms these logical dimensions are distinct from the 1046×830 renderer framebuffer. The shared emulator remains running at x1.

## Running the WASM emulator

- Opened the package-local browser shell on the shared port-8000 page and started the emulator. Confirmed the Start control changed to Pause, POWER is on, and BASIC readiness was detected at x1.

## Adding the WASM workload tab

- Added a Workload tab with a vertically oriented, newest-at-top graph, live average/peak summary, a 100% 10 ms budget line, and sampling around each WASM `emulator.advance()` call. Samples are retained for a three-second time window.
- Expanded the graph to fill the available tab panel on desktop and narrow screens. A browser test checks its size relative to the panel.
- Diagnosed the graph stopping at about 2.5 seconds on a high-refresh display: a fixed 300-frame cap represented only 2.5 seconds at 120 Hz. Replaced the sample-count cap with time-window trimming and added a 120 Hz regression test proving the full three-second history is retained.
- Validation passed: `cd dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser-tests && npx playwright test -c playwright.config.mjs layout.spec.mjs` (6/6), `typst compile --format pdf dev/src/4051-Emulator/rs/4051-Emulator-rs/docs/wasm.typ /dev/null`, and `git diff --check`. The shared browser remains running at x1 with the Workload tab selected. Playwright output is under ignored `target/`; the test-created `browser-tests/.npm-cache/` was removed.

## Fixing WASM Tools tab visibility

- Fixed a CSS specificity conflict where the workload tab's flex layout overrode the `hidden` state, causing its panel to remain displayed when Flash Drive or Controls was selected. Restricted the flex layout rule to the visible workload panel and added assertions that every unselected panel stays hidden.
- `npx playwright test -c playwright.config.mjs layout.spec.mjs` passed (6/6).

## Adjusting the WASM CPU speed selector

- Separated the CPU speed label from its native `<select>` and set both to 0.9rem; the native popup's visual placement still needs a cache-bypassing live-browser check.
- The user reported BASIC workload values of 100% at x20, 250% at x50, and 500% at x100. The graph measures `emulator.advance()` wall time against a 10 ms budget, so those values correspond to approximately 10 ms, 25 ms, and 50 ms per advance call, not CPU utilization.
- The focused WASM layout suite passed (6/6). The native popup's visual placement still needs a cache-bypassing live-browser check.

## Adding WASM performance benchmarks

- Added an opt-in Playwright benchmark for the running BASIC cursor prompt and a repeating vector-drawing loop at x1, x2, x5, x10, x20, x50, and x100. It reports three-second average/peak budget use, sample counts, timing coverage, and browser/viewport details without pass/fail performance thresholds.
- The manual run passed (1/1, 56.6 s). In this HeadlessChrome run, BASIC prompt/cursor averaged 104.55% at x20, 257.51% at x50, and 516.22% at x100; vector drawing averaged 123.08%, 304.11%, and 607.31% respectively. These are host-dependent `emulator.advance()` durations relative to 10 ms, not total browser CPU utilization.
- The regular layout suite passed (6/6) with the benchmark skipped by default. Typst compilation and `git diff --check` passed. Report output is under ignored `target/browser-input-tests/`.

## Running the WASM emulator

- Confirmed the repository-root static server responds with HTTP 200, opened the browser shell, and started the emulator. The Start control changed to Pause, POWER turned on, and BASIC prompt readiness was detected at x1. The Tools panel remains collapsed.

## Continuing WASM input-performance investigation

- Extended the opt-in Playwright benchmark with single-key browser-dispatch probes at x1, x20, x50, and x100 while BASIC is at its prompt. The report now includes whole-frame intervals, page-to-key-handler delay, and qualifying Event Timing entries; it distinguishes these from PIA delivery and BASIC echo.
- Headed Chrome 158, 1280x720, WebGPU vs forced CPU fallback: BASIC prompt/cursor at x100 averaged 1182.42% vs 491.55% of the 10 ms advance budget (about 118 ms vs 49 ms per advance). The vector loop was nearly equal at 548.04% vs 545.45%.
- Single key page-to-handler delay at x1/x20/x50/x100 was 4.2/73.2/203.9/338.2 ms with WebGPU and 2.4/29.6/76.7/151.1 ms with CPU fallback. These are one-event host-dependent dispatch samples; they do not measure key-to-PIA delivery or BASIC echo. WebGPU x100 had no Event Timing entry above the 16 ms reporting threshold, but the page-side probe recorded keydown and keyup.
- Source inspection found that `HostRuntime::sync_display` already submits one ordered GPU batch per machine batch, while WebGPU transient cursor geometry accumulates until the 100 ms refresh and is serialized/uploaded/redrawn in full on each dirty batch. Thus queue batching alone is unlikely to address the repeated geometry cost. Deduplicating identical marks per refresh interval is a candidate experiment, but no production renderer change was made.
- Both headed benchmark modes passed, as did focused layout/renderer/input tests (21/21), Typst compilation of the modified WASM/WebGPU documents, and `git diff --check`. Benchmark/build outputs are under ignored `target/`; `browser-tests/.npm-cache/` remains untracked. The earlier Rust Code Quality Reviewer run was cancelled before returning findings.

## Creating a browser-only WASM WebGPU improvement plan

- Added `dev/docs/tek4051/web/design/graded-wasm-webgpu-optimization-plan.typ` with grades WGP-0 through WGP-4: repeatable browser baselines, revision-aware presentation, lifecycle/fallback coverage, paired acceptance, and an explicit stop/defer gate for Rust-dependent fixes.
- The plan restricts implementation to WASM browser shell, browser tests, and directly related documentation. Rust emulator source, Cargo manifests, and Rust/WASM API changes are explicitly excluded.
- Updated the original renderer plan's current status and linked the follow-on plan from the architecture design and handoff.
- The new plan, updated renderer plan, and architecture design compiled with Typst; `git diff --check` passed. This planning task changed no Rust source.

## Validating browser-only WASM WebGPU presentation

- The first headed benchmark attempt reached the vector-program click while the Flash Drive panel was hidden. Made the opt-in benchmark explicitly select the Flash Drive tab and assert its panel is visible before clicking the imported row; no emulator or Rust code changed.
- Headed Chrome 158 at 1280x720, 2.9-second sample windows: BASIC prompt/cursor at x20/x50/x100 used 228.22%/561.42%/1145.48% of the 10 ms advance budget with WebGPU and 96.36%/241.17%/482.55% with forced CPU fallback. At x100 the vector loop was nearly equal (550.78% WebGPU, 544.73% CPU).
- At BASIC x100, WebGPU `present_webgpu()` averaged 0.056 ms for 9 calls, with 35 revision-based skips and 9 surface acquisitions. A single key's page-to-handler delay was 325.6 ms with WebGPU versus 147.6 ms with CPU fallback. These host-specific measurements show that redundant presentation is skipped, but presentation time cannot explain the remaining prompt-specific cost; synchronous advancement/display processing remains the likely area for further profiling. Rust-side work remains outside the authorized scope.
- Validation passed: headed WebGPU benchmark (1/1), forced-CPU benchmark (1/1), and focused headed input/layout/renderer suite (22/22). The test-created npm caches were removed. Playwright benchmark reports remain under Git-ignored `target/browser-input-tests/`.

## Completing repeatable WGP baselines and lifecycle checks

- Collected two additional headed WebGPU/CPU pairs in Chrome 158 at 1280x720, bringing the paired benchmark to three runs. BASIC prompt/cursor x100 measured 1119.15% [1114.30%, 1145.48%] of the 10 ms advance budget with WebGPU versus 487.08% [482.55%, 487.20%] with CPU fallback. At x50 the medians were 574.02% [561.42%, 578.46%] versus 244.85% [241.17%, 247.25%]; at x20, 229.22% [228.22%, 234.18%] versus 99.07% [96.36%, 100.32%].
- The x100 vector-loop medians remained close: 550.78% [550.28%, 551.39%] WebGPU and 544.73% [541.47%, 546.40%] CPU. WebGPU presenter mean was 0.062 ms [0.056, 0.063], with 8-9 actual presentations and 34-35 unchanged-frame skips per run. The presenter instrumentation arrived with the revision gate, so these are current-state skipped-call counts, not a direct pre-change timing comparison. Single-key page-to-handler delays varied substantially for WebGPU (median 325.6 ms, range 228.3-398.8 ms) and were more stable for CPU fallback (median 149.8 ms, range 147.6-151.2 ms).
- Expanded `renderer.spec.mjs` to verify redraw after changed revisions, resize, `visibilitychange`, and `pageshow`, plus CPU fallback when adapter initialization fails. A deliberately injected runtime surface failure could not be triggered through `GPUCanvasContext.getCurrentTexture` in this browser, so that probe was removed; runtime device-loss recovery remains unverified.
- Updated the graded WASM WebGPU plan with the repeated medians/ranges and grade status. WGP-0 and WGP-1 are complete; WGP-2/WGP-3 remain in progress. The three pairs support skipped-present savings but show no comparable improvement in synchronous advancement or input dispatch. The exact internal WASM cost remains unproven and Rust changes remain excluded.
- Validation passed: three WebGPU/CPU benchmark pairs (six benchmark runs, each 1/1), focused headed input/layout/renderer suite (23/23), `node --check` on the renderer spec, Typst compilation, and `git diff --check`.
