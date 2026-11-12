# Session history — 2026-11-21

## Reviewing vector-zoom toggle boundaries

- The user specified that machine `ScreenCopy` and snapshot APIs must remain unaware of camera zoom; no emulator-level or lower-layer behavior may change. UI screenshots capture the current camera view.
- The user approved two UI controls for initial testing: a master vector-zoom feature toggle on the Test tab, and (only when enabled) a main-menu selector between raster and vector zoom. With the master toggle off, existing bitmap zoom remains and vector-scene recording/rendering adds no overhead. With it on, capture all supported visible scene content and preserve live glow.
- Performance and scene-memory costs should be measured and reported after implementation; the user will decide how to proceed from those results. No code was changed.
- Review note: switching the master feature on after drawing cannot reconstruct earlier vectors without retained history. For the initial test workflow, enable vector zoom before drawing the test scene, or clear/rerun the scene after enabling. While enabled in raster mode, scene recording must remain active to support switching immediately to vector mode.

## Making vector-zoom activation a hard cut

- The user specified that enabling the Test-tab master toggle resets the machine as an intentional hard cut.
- This establishes a clean scene-capture boundary and removes the need to reconstruct geometry from before feature activation. The UI should make the reset side effect explicit; implement it by invoking the existing reset behavior, not by changing machine/emulator semantics.
- Start the scene recorder at the reset boundary so post-reset visible content, including live display activity, is captured. Turning the master toggle off still stops and releases vector-only work; no implementation code changed.

## Considering an implementation plan

- Recommended a lightweight staged plan before implementation because the work crosses renderer ownership, scene recording, glow timing, browser controls, and a reset-on-enable lifecycle.
- Proposed stages: reconcile contracts and acceptance; establish the feature gate/reset and capture lifecycle; implement vector rendering and camera switching; preserve live glow; validate UI and Machine/ScreenCopy isolation; then measure memory/performance on matched workloads and decide whether to proceed. No plan artifact or implementation code was created.

## Creating the vector-zoom implementation plan

- Deleted the user-requested `dev/images/202611202108/file.png`.
- Created `dev/docs/tek4051/web/design/vector-zoom-implementation-plan.typ` from the clarified conversation requirements, with staged tasks, requirement mapping, renderer/Machine boundaries, toggle/reset lifecycle, glow behavior, browser/Rust validation, and post-implementation performance/memory reporting.
- Updated the graded renderer plan to reconcile its former UI exclusion and linked the new plan from the design index.
- Compiled all three changed Typst documents successfully. Temporary PDFs were removed; no implementation code changed.
- The user confirmed that enabling the master feature starts in Raster mode; Vector mode requires explicit selection.

## Implementing optional vector-scene capture

- Added a renderer-independent ordered scene recorder for stored/transient marks and vectors, event-time granularity, refresh and erase lifetimes, ScreenCopy non-mutation, dropped-event errors, fallible scene-buffer growth, and approximate current/peak retained-byte accounting.
- Integrated capture as an opt-in Extended WebGPU sidecar. The WASM API rejects CPU-only activation, arms capture before invoking the existing reset path, detects renderer-loss state from the active runtime rather than a stale local flag, and releases scene storage on disable without resetting the machine.
- Added Rust tests for all semantic event categories, ordering, refresh/erase/ScreenCopy behavior, feature-off inactivity, memory accounting, successful hard-cut activation, disable without resetting later machine state, and CPU-only rejection. The code and test reviewers' findings were resolved and confirmed on follow-up.
- Validation passed: WASM-feature library tests (58 passed, 4 ignored), WASM API tests (12 passed), native library/emulation tests (149 passed, 1 ignored; 7 integration tests passed), `cargo check` for `wasm32-unknown-unknown`, Clippy, Rust formatting, and the updated Typst plan compilation. Native tests printed compiler warnings and a non-failing ZIP writer diagnostic.
- The performance review is inconclusive: release builds passed for native CPU/WebGPU and WASM, but no runtime renderer was verified and no matched capture-off/on timing was available. WASM binding regeneration was blocked because `wasm-bindgen` is not installed; no timing claim is made.
- The implementation plan now marks scene representation, event integration, lifecycle, and focused tests complete. Test-tab controls, the Raster/Vector selector, vector rerendering through the shared camera, glow-preserving mode changes, browser acceptance, and matched performance/memory reporting remain next.

## Continuing vector-zoom implementation controls

- Added the Test-tab vector-scene capture master checkbox. It starts off, is enabled only while Extended WebGPU is active, clearly labels the reset-on-enable behavior, calls the existing BrowserEmulator capture/reset API, and releases scene capture without resetting when switched off. If Extended WebGPU is lost, the UI attempts to release capture and reports failures.
- Added browser coverage for CPU-fallback unavailability and Extended WebGPU enable/disable behavior. The CPU-fallback assertion passed; the Extended WebGPU tests were skipped because the available browser did not initialize WebGPU.
- Updated the WASM API guide and marked plan tasks T004/T005 complete. No Raster/Vector selector or camera-driven vector presentation was added.
- Validation passed: WASM library build, WASM API tests (12 passed), renderer browser tests (3 passed, 3 skipped), display-camera/layout tests (9 passed), JavaScript syntax checks, and Typst compilation of the plan and WASM guide.
- `wasm-bindgen-cli` 0.2.130 was installed only under the ignored workspace `target/tools/` directory to regenerate ignored browser bindings under `target/wasm/pkg/`; no dependency manifests changed. These generated/build artifacts are Git-ignored.
- Next: implement the presentation-only Raster/Vector selector and vector rerender through the floating camera, preserving live-glow age and keeping Machine ScreenCopy/snapshot sources camera-independent. Extended-WebGPU runtime validation remains unavailable in this browser.

## Exposing the shared camera transform

- Extended `bindDisplayCamera` to report zoom and pan in logical display coordinates, notify a caller when the camera changes, and suppress the WebGPU canvas's CSS transform when vector presentation is selected. Raster mode retains the existing CSS transform.
- Added Playwright coverage for logical-unit pan conversion, camera-change notifications, and avoiding double CSS zoom. The focused camera suite passed (3 tests); JavaScript syntax checks passed.
- The combined renderer/camera/layout regression run passed 13 tests and skipped 3 WebGPU-dependent tests because the browser did not initialize WebGPU; the updated implementation plan compiled successfully.
- This is camera-interface groundwork only: no caller selects vector presentation yet, no Extended-WebGPU vector rerender has been connected, and no new GPU behavior or performance claim is made.
- Next: pass camera updates through a WASM presentation API, implement separate vector presentation/composition while preserving age-correct glow and camera-independent ScreenCopy, then expose the Raster/Vector selector.

## Connecting vector presentation and mode controls

- Added a separate, lazily allocated Extended-WebGPU vector presentation texture. Retained stored geometry is rerasterized through the camera, while the original age-stamped glow is sampled through the inverse camera transform; the camera-independent raster output remains the Machine ScreenCopy source.
- Added WASM mode/camera APIs, browser source rebinding, and a main-menu Raster/Vector selector. Capture still starts in Raster mode, is reset-on-enable, and releases vector-only resources when disabled. Camera updates do not advance emulation.
- Added native GPU tests for rerasterized zoom/pan, complete ScreenCopy pixel isolation, supported zoom endpoints and invalid inputs, and live/expired glow age through camera and mode changes. The test reviewer requested stronger full-frame ScreenCopy and camera-boundary assertions; these were added and follow-up review confirmed both findings resolved with no remaining meaningful gaps.
- Validation passed: 11 Extended WebGPU renderer tests, 12 WASM API tests, native and wasm32 WebGPU checks, Clippy, Rust formatting, regenerated WASM bindings, and Typst compilation of the plan and WASM guide. Renderer/camera browser tests passed 6 and skipped 3 WebGPU-dependent cases because the browser does not initialize WebGPU.
- Code-quality review found no issues. Performance review is inconclusive: native CPU/WebGPU and WASM release builds passed, but there are no matched vector-zoom runtime measurements, and no browser adapter was available. No performance pass or product decision is claimed.
- Generated bindings, build/test output, and compiled Typst PDFs are under Git-ignored `target/`; no dependency manifests changed. Next: finish browser acceptance on a WebGPU-capable browser, then measure matched feature-off, enabled/Raster, and enabled/Vector workloads and scene memory.

## Starting the release WASM Extended WebGPU emulator

- Built the optimized WASM library with `cargo build --locked --release --target wasm32-unknown-unknown -p tek4051-emulator --no-default-features --features wasm --lib` and regenerated web bindings with `target/tools/bin/wasm-bindgen` 0.2.130.
- Reused the existing local server at port 8000 and opened `?renderer=extended&build=20261121-0321-vectorzoom` (page ID `3cb5bd44-d841-4dc4-8ec2-d63ad7b9d04e`). The page reports Extended WebGPU active, BASIC prompt detected, x1, POWER on, and running; vector-scene capture remains off by default.
- Release WASM and generated bindings are Git-ignored under `target/`. No source files or dependency manifests changed in this chat.

## User feedback on vector zoom

- The user reports that vector zoom works fluently with no noticeable performance degradation on the active release Extended-WebGPU setup.
- Record this as qualitative user feedback only; matched workload timing and memory measurements remain open.

## Checking BASIC test latency and benchmark requirements

- Investigated why the concentric-circle Playwright test ran for over ten minutes. The input sequencer holds each character for 50 ms plus 30 ms release; the 47-line literal scene has about 785 submitted characters, or about 63 seconds of minimum key phases before per-line BASIC prompt detection. The headless Chromium renderer remained active, so it was not stalled, but the total time is not explained by key timing alone.
- Confirmed CPU speed changes affect emulation workload, not the sequencer's elapsed-time press/release phases; x100 is not a valid shortcut and can increase frame work. Removed the test-only x100 selection.
- Compact trig/array BASIC variants returned to a prompt without producing geometry (0–80 lit pixels). The Concentric circles scene remains unverified and is excluded from benchmarks until corrected; no benchmark was run.
- Updated the vector-zoom plan to require preflighted supplied Flash Drive and generated long BASIC programs, explicitly exclude failing supplied programs, and measure feature-off/enabled-Raster/enabled-Vector at x1, x2, x5, x10, x20, x50, and x100. Added elapsed/workload/frame-time, CPU scene memory, and GPU buffer/texture reporting with estimates labeled.
- Validation passed: JavaScript syntax checks, `typst compile` for the plan, and `git diff --check`. Generated Playwright diagnostics and the compiled plan PDF are under Git-ignored `target/`.
- Next: fix the BASIC circles test and establish a reliable execution-start/completion signal before implementing/running the long-program benchmark matrix.

## Building BASIC geometry diagnostics and starting release WASM — 2026-11-21

- Corrected Flash Drive test-file metadata padding and validated 20 horizontal, vertical, angled, and fixed-random lines, then 20 nested centered triangles, 20 nested centered rectangles, and 20 nested centered rectangles rotated 45 degrees. The geometry test confirmed every scripted segment in both the framebuffer and a smoothed CSS-downscaled view.
- Added an explicit 24-segment circle `.bas` diagnostic. The prior circle UI test still reported only 80 lit pixels, and the Flash Drive circle attempt had no expected segment probes after 30 seconds; the cause remains unresolved. A trial that typed a MOVE before every segment exceeded the 240-second UI test deadline, so the UI helper was restored; the static Flash Drive circle uses explicit MOVE/DRAW pairs and its latest run was interrupted before completion.
- Started the optimized WASM page at `http://127.0.0.1:8000/dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser/?renderer=extended&build=20261121-0321-vectorzoom`. Verified Extended WebGPU active, x1, POWER on, and BASIC prompt ready. The emulator remains running.
- JavaScript syntax and whitespace checks passed. The geometry test passed for all four line cases and the three nested-shape cases in 1.6 minutes; circle coverage remains unverified. Playwright output under `target/` is Git-ignored.
- Next: resume the focused circle diagnosis without running a long test concurrently with the live release page; then return to correctness-preflighted arithmetic and long-program benchmarks.

## Creating FAN-100 and starting release Extended WebGPU — 2026-11-21

- Added `29     ASCII   PROG [FAN-100] 3872.bas` under the WASM test scripts. It initializes and clears the display, then issues 100 explicit `MOVE 0,0` / `DRAW x,y` pairs spanning the visible first quadrant. Four-decimal endpoints keep each BASIC-coordinate ray at length 20 without relying on BASIC trig conventions.
- Structural validation passed: 100 moves, 100 draws, every origin is `(0,0)`, endpoint-radius error is at most 0.000055, CR-only records, and the filename byte count matches the 3,872-byte fixture. No emulator run of FAN-100 was performed. The source file is not ignored by Git.
- Rebuilt the optimized WASM library and regenerated web bindings with the existing workspace-local `wasm-bindgen`. Reused the responsive server on port 8000 and opened `?renderer=extended&build=20261121-1655-release`; after Start, the page reported Extended WebGPU, x1, POWER on, and BASIC prompt detected. The emulator is left running for the user. Build and binding outputs under `target/` are Git-ignored.
- H-LINES remains unresolved: its Flash Drive run was previously observed to emit three vectors. The direct BASIC-sequencer diagnostic did not return to a prompt within 100 seconds of simulated time and stopped with 288 input transitions pending. The renderer erase change is not established as the cause or fix.
- Next: leave the active release page available; when continuing diagnosis, isolate the H-LINES load/termination state without running Playwright against the live page, then exercise FAN-100 once execution completion is reliable.

## Adding a 100-ray BASIC fan fixture

- Added `29     ASCII   PROG [FAN-100] 3872.bas` under the WASM test scripts. It initializes and clears the display, then issues 100 explicit `MOVE 0,0` / `DRAW x,y` pairs spanning the first quadrant; four-decimal endpoints keep each BASIC-coordinate ray at length 20 without relying on trig conventions.
- Structural validation passed: 100 moves, 100 draws, every origin is `(0,0)`, endpoint-radius error is at most 0.000055, CR-only records, and the filename byte count matches the 3,872-byte file. No emulator/browser run was performed for this fixture.
- H-LINES remains unresolved: the Flash Drive run was previously observed to emit three vectors. The direct BASIC-sequencer diagnostic still did not return to a prompt after its 100-second simulated-time bound; it stopped progressing with 288 input transitions pending. The renderer erase change is not established as the cause or fix.
- The new `.bas` file is not ignored by Git. Next: continue isolating the H-LINES load/execution failure, then run the fan through the emulator once the execution/completion signal is trustworthy.

## Varying every tenth FAN-100 ray

- Updated the FAN-100 fixture so zero-based ray indices 0, 10, ..., 90 have length 105 and the other 90 rays remain length 100. Structural validation passed for 100 origin-based MOVE/DRAW pairs, the endpoint radii, CR-only records, and the 3,929-byte filename metadata; the source is not ignored by Git.
- The active release page still contains the prior all-100-unit file. Its Flash Drive displayed one stored file after the prior import, and an open “Clear all files from the in-memory Flash Drive?” dialog prevented the attempted restoration of the nine local test scripts. The dialog was not confirmed, and this updated fixture was not imported or run.
- H-LINES remains unresolved; preserve the active page state until the user directs how to handle the dialog and Flash Drive contents.

## Creating a loop-based fan script

- Added `30     ASCII   PROG [FAN-LOOP] 281.bas`, which uses nested `FOR` loops and variables with degree-based `SIN`/`COS` to reproduce FAN-100: indices 0, 10, ..., 90 have length 105; the other 90 rays have length 100. The static fixture's 100 angles and radii were checked against this pattern, and the script's CR-only records and filename byte count were validated. The source is not ignored by Git.
- The user reports the static fan drew all rays, with indices 80 and 90 cropped by the 100-unit screen height. The loop script was not imported or run; the active page still has the earlier file and an unresolved Flash Drive clear-confirmation dialog.

## Confirming FAN-LOOP and creating H-STACK

- The user confirmed that FAN-LOOP draws the same line pattern. This is user-reported runtime confirmation; indices 80 and 90 still extend beyond the 100-unit screen height.
- Added `31     ASCII   PROG [H-STACK] 91.bas`: a five-iteration BASIC loop draws horizontal lines from x=0 to x=9 at y=0, 1.8, 3.6, 5.4, and 7.2. Structural checks passed for the line count, spacing, CR-only records, and filename byte count. The source is not ignored by Git; emulator execution is unverified.
- The shared page's current Flash Drive inventory and dialog state were not rechecked. An earlier import showed one stored file instead of nine, and a clear-confirmation dialog blocked the attempted restore; do not assume the other fixtures are present.

## Confirming H-STACK output

- The user reports all five H-STACK lines were drawn in the emulator. This is user-reported runtime confirmation; the shared page inventory and confirmation-dialog state were not independently inspected.

## Creating combined HV-STACK

- Added `32     ASCII   PROG [HV-STACK] 160.bas`, preserving H-STACK's five horizontal length-9 lines and adding five vertical length-9 lines at x=10, 11.8, 13.6, 15.4, and 17.2, each from y=0 to y=9. Structural checks passed for all ten lines, 1.8-unit spacing, CR-only records, and the filename byte count. The source is not ignored by Git.
- The combined script has not been imported or run; the current shared page state was not checked.

## Investigating browser click responsiveness

- During the report, the page showed `Auto Load: Typing file-1 commands` and BASIC prompt not detected. The Flash Drive toolbar, pickers, and number field were disabled; `flash_drive.js` explicitly disables them while Auto Load is `Typing`.
- A browser-tool click successfully selected the Controls tab. On a later snapshot, the BASIC prompt was detected and Flash Drive controls were enabled again. This supports a temporary Flash Drive control lock during Auto Load, not a browser-wide hang. Native physical mouse input was not independently tested.
- The latest snapshot listed files 30, 31, and 32 at x2 with Vector zoom and Extended WebGPU. The earlier restoration issue remains unresolved.

## Creating alternating HV-CHECK

- The user clarified the 10×10 checkerboard cell origins: H at `(0,0)`, V at `(10,0)`, V at `(0,10)`, H at `(10,10)`, alternating thereafter. Created `33     ASCII   PROG [HV-CHECK] 401.bas` with 50 H-stacks and 50 V-stacks; each stack has five length-9 lines spaced by 1.8 inside a 10×10 cell. Structural validation passed for all 500 segments, bounds `[0,99]`, CR-only records, and filename byte count. The source is not ignored by Git; emulator execution is unverified.

## Extending HV-CHECK with long parallel lines

- Added `34     ASCII   PROG [HV-LINES] 539.bas`, retaining HV-CHECK's 500 checkerboard segments and adding 20 horizontal length-100 lines at y=100..119 from x=0, plus 20 vertical length-100 lines at x=100..119 from y=0. Structural validation passed for 540 total segments, exact one-unit spacing, CR-only records, and filename byte count. The source is not ignored by Git; emulator execution is unverified.

## Creating RECT-140

- Added `35     ASCII   PROG [RECT-140] 98.bas`, which draws a closed 140-by-100 rectangle with its corner at `(0,0)`. Checks passed for the four segment lengths and endpoints, CR-only records, and filename byte count. The source is not ignored by Git; emulator execution is unverified.

## Fixing disabled Flash Drive controls and updating the rectangle

- The user observed disabled Flash Drive buttons. The page snapshot showed Auto Load in `Typing`; this state intentionally disables Flash Drive operations while queued BASIC commands are being entered. Added a Cancel Auto Load button visible during typing; it calls the existing `release_input()` API to cancel the sequence and allows directory operations again, with a notice that a partial BASIC command may remain.
- The focused Playwright test passed: `npx playwright test --config dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser-tests/playwright.config.mjs dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser-tests/flash-drive.spec.mjs --grep "cancels Auto Load"`.
- The full Flash Drive spec ran 13 tests: 12 passed, 1 failed in the Pi benchmark RTC test. Re-running that test alone reproduced the failure: after `FIND@5:10`, the test expected the BASIC readiness indicator to transition to not detected, but it remained detected. The status indicator is derived from the emulator snapshot; the failing path did not invoke the new cancel handler. This remains a separate follow-up, not a confirmed regression from the cancellation change.
- The shared page's last inspected snapshot had Auto Load Sent, BASIC ready, and Flash Drive controls enabled. It was not reloaded, so the new Cancel button is not yet active in that tab; a reload could lose its in-memory display and Flash Drive state. Native physical mouse input was not independently tested.
- The user changed the rectangle width from 140 to 139 and then to 120. Final fixture: `35     ASCII   PROG [RECT-120] 98.bas`, a closed 120-by-100 rectangle at the origin. Structural checks passed; it is not ignored by Git and has not been run in the emulator.

## Creating RECT-120-H25

- Added `36     ASCII   PROG [RECT-120-H25] 176.bas`, retaining RECT-120 and adding 25 horizontal lines. A loop starts each at x=0, with y=25..49 and lengths 120..144, each increasing by one. Verified the loop bounds, corresponding coordinates, CR-only format, filename byte count, and that Git does not ignore the file.
- The fixture has not been imported or run in the shared browser. The prior live-page snapshot showed the earlier files; preserve its in-memory state before reloading or importing.

## Resolving browser file import

- The user reported that files could not be imported. Before reloading, the shared page showed 0 stored files and an enabled Import button. The user reloaded the window and reported that importing then worked.
- A read-only snapshot afterward showed BASIC ready, one stored file (number 36, `RECT-120-H25`, ASCII PROG, 176 bytes), a successful import notice, Auto Load commands sent, and a program launch request. The geometry was not visually verified; the live page was left untouched.

## Creating RECT-120-H25-V100

- Added `37     ASCII   PROG [RECT-120-H25-V100] 240.bas`, retaining RECT-120 and the 25 horizontal lines from RECT-120-H25, then drawing vertical lines at x=129 and x=130 from y=0 to y=100.
- Verified the source commands, CR-only records, 240-byte filename metadata, and that Git does not ignore the file. The new fixture was not imported or run; the shared browser's in-memory state was left untouched.

## Creating RECT-130

- Added `38     ASCII   PROG [RECT-130] 98.bas`, drawing a closed rectangle 130 units wide and 100 units high from the origin.
- Verified the four edges, CR-only records, 98-byte filename metadata, and that Git does not ignore the file. It has not been imported or run in the emulator.
- The user reports that the rectangle appears to exactly fit the screen; the coordinate boundary was not independently measured.

## Creating RECT-130-HATCH45

- The user specified true 1-unit perpendicular spacing and accepted positive 45° slope. Added `39     ASCII   PROG [RECT-130-HATCH45] 361.bas`, retaining the 130×100 rectangle and generating 163 clipped parallel hatch lines. The perpendicular gap is 1 unit; a centered phase balances the border margins.
- Geometric checks confirmed every endpoint stays within the rectangle and every segment has a positive 45° slope; verified CR-only format, 361-byte filename metadata, and that Git does not ignore the file. It has not been imported or run in the shared browser.

## Commenting BASIC fixture groups and starting release WASM

- Added logical-group `REM` comments throughout all 20 BASIC fixtures (programs 21-40), covering scaling/drawing loops, stored DATA, fan-ray ranges, checkerboard helpers, and hatch clipping groups. Executable program records were preserved, and all filename byte counts were refreshed.
- Static checks passed: ASCII content, CR-only records, strictly increasing BASIC line numbers, matching filename byte counts, and no Git-ignored fixtures. `git diff --check` passed. The fixtures were not executed as part of this comment-only change.
- Opened the release page at `?renderer=extended&build=20261121-1655-release`; Extended WebGPU is active at x1, the machine is running with BASIC prompt detected, and the Flash Drive is empty. The page has not loaded or launched a geometry fixture.
- The previous missing-hatch snapshot contained script 39, which draws only the positive-slope family; script 40 contains both families. Next: import and launch script 40 if the hatch issue is being checked.

## Investigating Spacelab import latency

- The Spacelab folder contains 11 files totaling 441,782 bytes. The shared release page later showed `Processed 11 selected file(s)`, 11 stored files, Extended WebGPU, x100 selected, and the machine stopped.
- Added a Playwright regression importing the real folder. It completed in 61 ms from file assignment through success status (296 ms test body; 879 ms total run) and verified all 11 directory rows. No hang reproduced.
- The import path reads/prepares files in a JavaScript worker and stores them via the Flash Drive import API; emulator speed does not accelerate file I/O. The original delay's exact cause remains unknown because it was not timed. If it recurs, time from file-picker closure and capture the processing status and browser console.

## Preserving vector stroke width while zooming in

- Capped Extended WebGPU stroke-width scaling at 1× while leaving geometry transforms and zoom-out thinning unchanged. Regression assertions cover widths and endpoints at 0.5×, 1×, and 2×.
- All 13 `emulation::extended_webgpu::tests` passed; `cargo fmt --all -- --check`, `git diff --check`, and Typst compilation passed. Release WASM was rebuilt and rebound with local `wasm-bindgen` 0.2.130.
- Browser verification on page `85fc7685-a312-4f40-9401-6f870dfdc8cc` confirmed Extended WebGPU, Vector selection, and no CSS canvas transform. The latest page snapshot showed x50, 91 stored files, and file 37 selected, so the page/storage was left untouched. The Test-tab explanatory status had remained stale at “Raster” despite Vector being active.
- Performance review is inconclusive: release build configurations succeeded, but there was no matched baseline/candidate runtime measurement of vector-zoom rerasterization. Generated WASM under `target/wasm/pkg/` is Git-ignored.
