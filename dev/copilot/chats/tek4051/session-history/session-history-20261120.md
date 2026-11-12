# Session history — 2026-11-20

## Starting release WASM with WebGPU

- Opened a fresh browser page using the existing release WASM artifacts at `http://127.0.0.1:8000/dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser/?release-webgpu=20261120-0107`.
- Verified the WebGPU renderer initialized, started the emulator, and confirmed the BASIC prompt was detected with POWER on. The emulator remains running at x1.
- Current browser page ID: `c9c5d718-f815-4178-a479-23ca12aaef6c`. No source code or build artifacts were changed.

## Refining the Controls tab front panel

- Replaced the ten combined user-key buttons with 20 separate buttons, numbered 1-20 in row-major order in a four-by-five grid under “User Definable.” Keys 1-10 use their unshifted Tek codes; keys 11-20 use the same codes with a fixed Shift override, independent of host Shift state.
- Changed User Definable Keys, Line Editor, and Program and special into vertically stacked, full-width groups with headings above their controls.
- Added focused browser coverage for the user-key code/Shift mapping, 4x5 grid and numeric order, responsive narrow-screen layout, full-width group stacking, and preserving event-time Shift behavior for other panel controls.
- Validation passed: `npx playwright test -c playwright.config.mjs input.spec.mjs layout.spec.mjs` (21/21) from `platforms/wasm/browser-tests`; `node --check` passed for the modified JavaScript files. The cache-busted preview page at `?user-keys-vertical=20261120-0125` shows the updated stacked layout with WebGPU active; the machine is stopped there. The original release page remains running at x1.
- Playwright output is under Git-ignored `target/`. No Rust sources or build artifacts were changed.
- Renamed the user-key group heading from “User Definable Keys” to “User Definable”; it remains an H3 and labels its accessible group. Updated the layout assertion accordingly.

## Splitting the Line Editor buttons

- Replaced the five combined Line Editor buttons with ten distinct buttons in a two-row, five-column grid. The top row selects shifted actions (Compress, Rub Out Left, Rub Out Right, Reprint, Next Line); the bottom row selects unshifted actions (Expand, Back Space, Space, Clear, Recall Line).
- Each pair keeps its original Tek key code and sets an explicit Shift override. The existing panel-key input behavior and User Definable grid remain unchanged.
- Validation passed: the focused input/layout Playwright suite passed (21/21), including button order, mapping, and grid assertions; `git diff --check` passed.
- The cache-busted preview at `?line-editor-grid=20261120-0135` shows the updated two-row grid with WebGPU active. The preview machine is stopped; the original release page remains running at x1. Playwright output is under Git-ignored `target/`.
- Renamed the top-right Line Editor button to “Recall Next Line” and inserted a visible line break between “Recall” and “Next Line.” Added assertions for both its accessible name and rendered line break.
- The focused layout test passed (1/1), and the active preview at `?line-editor-label=20261120-0139` displays the requested line break. `git diff --check` passed.

## Discussing the Extended WebGPU renderer

- The user named the proposed renderer the Extended WebGPU renderer. Reviewed the current renderer boundaries: `CpuReferenceRenderer::refresh` removes cursor-level pixels while retaining stored vectors/dots; `WebGpuVectorRenderer::refresh_display` clears transient geometry while the stored GPU layer persists. The emulation runtime schedules display refresh every 100 ms.
- Recommended a separate, opt-in Extended WebGPU renderer type and selection path, keeping the CPU reference and current WebGPU implementation/default path unchanged. A candidate aging model is a GPU ping-pong intensity field with elapsed-time decay followed by drawing new strokes, while preserving erase, capture ordering, and semantic readiness behavior. Avoid running both renderers on the normal path; benchmark the new renderer separately against matched baseline workloads.
- No source changes or design decisions were made. Remaining design choices include fade-to-zero versus a persistent dim floor, whether phosphor ages while paused, and the decay timebase/rate. Next step, if implementation is requested, is to agree on those semantics and build a deterministic decay/erase test corpus before coding.

## Specifying phosphor-aging timing

- The initial target is elapsed-time-based fade to black, with a nominal 1-second duration at x1 and inversely scaled duration at higher CPU speeds (`duration = base_duration / speed_multiplier`). The selected speed multiplier continues to apply while paused. At the initial x100 baseline, the duration is 10 ms of host elapsed time.
- The user clarified that both base duration and decay curve may change; linear was only the initial model, and exponential or inverse-power curves are also acceptable. Asymptotic curves need an explicit render-intensity threshold for black.
- Because the initial x100 duration is shorter than the existing 100 ms display-refresh interval, decay must use actual elapsed deltas at a sufficiently frequent update/presentation point. Keep curve parameters and evaluation local to the Extended WebGPU renderer; do not add UI/configuration or modify existing renderer paths for tuning.
- No source changes or renderer implementation were made. Validation should cover decay progression and black threshold, speed scaling, paused decay, rebrightening on redraw, ERASE, cadence, and performance.

## Implementing the Extended WebGPU renderer

- Added a distinct `ExtendedWebGpuRenderer` with a per-pixel hit-time/coverage texture and elapsed-time presentation shader. Linear one-second decay at x1 is the default; speed scales decay during active, paused, and stopped host advancement. Exponential and inverse-power curves use validated thresholds and duration bounds. Existing WebGPU renderer code and default selection remain intact.
- Fixed review findings: clear phosphor stamps with zero alpha, preserve recent strokes over modular clock wrap, mark cleared frames dirty, propagate runtime speed both before and after renderer installation, and document paused/stopped phosphor advancement. The renderer speed setter is crate-private.
- Added opt-in WASM selection using `?renderer=extended`, renderer status labeling, and a package-local guide. ERASE flash/completion and native screen-copy order are preserved; browser screen copies explicitly use CPU reference pixels because synchronous WebGPU readback is unavailable.
- Focused GPU/runtime tests cover off-stroke black, linear midpoint intensity, two thresholds for exponential and inverse-power curves, invalid parameter bounds, tick wrap, speed selection before/after installation, pause decay, redraw/rebrightening, ERASE, and screen-copy ordering. The WebGPU-enabled library suite passed (166 passed, 7 ignored); focused renderer/runtime tests passed (6/6 and 2/2).
- Release WASM build and wasm-bindgen 0.2.130 succeeded. Renderer Playwright tests passed 3 and skipped 2 because that browser lacks WebGPU; the integrated release page loaded the updated bindings, initialized Extended WebGPU, reached the BASIC prompt, and remains running at x1 without a runtime alert. Strict all-target WebGPU Clippy, formatting, and `git diff --check` passed. Build and Playwright outputs are Git-ignored under `target/`.
- Rust Code Quality and Test Quality reviews found no remaining actionable issues. The performance review is inconclusive: no matched native or WASM renderer timing comparison was available. Three shared-runtime Battlestar benchmark differences were -3.2%, +1.1%, and -1.3% with overlapping ranges; this workload is not renderer evidence. The review also found unguarded Extended-runtime calls in native CPU-only builds; both calls now have the `webgpu` feature gate.
- After that fix, native CPU-only Release, native WebGPU Release, and WASM Release builds all passed. The no-default-features library tests passed (24), strict Clippy passed with and without WebGPU, and the final WebGPU library suite passed (166 passed, 7 ignored). Release bindings were regenerated with wasm-bindgen 0.2.130. `renderer.spec.mjs` passed 3 tests and skipped 2 because its Playwright browser lacks WebGPU; the integrated release browser loaded the updated Extended renderer at the BASIC prompt. Formatting and `git diff --check` passed. Generated WASM, bindings, and Playwright output remain Git-ignored under `target/`.

## Starting release WASM with Extended WebGPU

- Opened the release page at `http://127.0.0.1:8000/dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser/?renderer=extended&build=20261120-final` and started the emulator. It reports Extended WebGPU, reached the BASIC prompt at x1 with POWER on, and showed no runtime alert.

## Decaying glow to normal WebGPU intensity

- Changed Extended WebGPU composition from fading strokes to black to rendering a decaying highlight over an unchanged standard WebGPU baseline. New stroke masks reach full green, then the selected curve decays the excess brightness to the standard renderer's exact per-pixel intensity. The standard renderer remains unchanged and is still the default.
- Routed display events and 100 ms transient refresh through the baseline renderer, retaining stored output and normal cursor-removal behavior. Added direct GPU pixel comparisons against `WebGpuVectorRenderer`, plus tests for full-green peak, midpoint decay, transient refresh, ERASE, screen-copy ordering, speed, and pause.
- Focused Extended renderer tests passed (7), runtime tests passed (2), and the WebGPU library suite passed (167 passed, 7 ignored). No-default-features tests passed (24). Strict WebGPU Clippy, release WASM build/binding, formatting, and whitespace checks passed. Renderer Playwright tests passed 3 and skipped 2 because the Playwright browser lacks WebGPU; the integrated release browser loaded the update and ran Extended WebGPU at the BASIC prompt.
- Code Quality, Test Quality, and performance reviews are pending. Performance status remains inconclusive without matched renderer timing; browser screen-copy pixel parity and broader device acceptance remain open. Generated WASM, bindings, and Playwright files are Git-ignored under `target/`.

## Brightening Extended WebGPU glow toward white

- Changed only the Extended WebGPU composition shader: a fresh full-coverage stroke now peaks at RGB approximately (242, 255, 242), then its red/blue tint and green excess decay to the unchanged standard WebGPU RGBA frame. The default renderer and its rendering path remain unchanged.
- Strengthened the convergence regression to compare the complete RGBA frame against standard WebGPU, verify near-white peak channels and fading tint, and check that transient glow survives the normal refresh. The focused test and full WebGPU-enabled library suite passed (167 passed, 7 ignored); strict WebGPU Clippy, formatting, and whitespace checks passed.
- Release WASM build and `wasm-bindgen` regeneration succeeded. Reloaded the integrated browser page and confirmed Extended WebGPU is running at x1, BASIC prompt detected, POWER on, and no runtime alert. The generated WASM and bindings under `target/` are Git-ignored.
- Code Quality and Test Quality reviews found no remaining issues. Performance review is inconclusive: no matched baseline/candidate renderer measurements were collected, and the near-white tint itself was not benchmarked. Browser screen-copy pixel parity and broader device acceptance remain open.

## Adding the WASM BASIC display tests

- Completed the Tools > Test tab with Grid, Snake fill, and Concentric circles programs. Each program starts with `PAGE`, ends with `END`, and numbers lines from 100; the UI warns that existing program lines there may be overwritten.
- The grid now draws both axes, the snake program traverses alternating rows with a connecting vertical segment, and circles use five nested radii with 24-point trig/array loops matching syntax from the supplied ROM program. Early candidates returned to the prompt without visible geometry; the browser regression's pixel assertion exposed this, and the final versions pass.
- The real-WASM browser regression passed (1/1, about 1.7 minutes), requiring more than 1,000 lit pixels and a returned BASIC prompt after each test. Four-tab layout tests passed (7/7). The WASM API suite passed (10/10), JavaScript syntax and `git diff --check` passed, and the updated `wasm.typ` compiled.
- The Rust Test Quality Reviewer remained running after a bounded wait; manual review of the pending-input API test and its session forwarding method found no actionable issue. Generated Playwright, WASM, and Typst outputs are Git-ignored under `target/`.
- The tests can overwrite existing BASIC lines 100 onward and do not restore the prior listing. A future non-destructive approach needs an explicit save/restore design.

## Fitting the WASM BASIC grid to the canvas

- The user asked to try grid scale factors `0.1`, `0.01`, and `0.001` to determine whether the missing lines were outside the visible range. The `0.1` trial produced all ten grid lines but left the grid undersized; `0.01` drew a small visible corner grid and fell below the strong-band threshold; `0.001` produced only 159 lit pixels.
- Selected a `0.1265` BASIC coordinate scale. Strengthened the browser regression to require ten strong lines per axis, even spacing, and all four borders within 7% of the canvas edges. The grid, snake-fill, and concentric-circle Playwright test passed (1/1, about 2 minutes), including the blue clear background and zero blue draw pixels.
- The public WASM API diagonal geometry test passed; the Rust Test Quality Reviewer found no issues. `node --check` for the modified JavaScript and `git diff --check` passed.
- An expanded 42-line grid program exceeded the browser test's 12-minute completion timeout and was replaced with the compact loop program. Browser runs created Git-ignored files under `target/`.
- Remaining related follow-up: the Test tab still overwrites BASIC lines starting at 100 and does not restore them; interruption/blur handling should also be hardened so a canceled run cannot show success.
- A verbose scan of the final Playwright image reports ten vertical centers (`18, 130, 242, 354, 466, 579, 690, 802, 915, 1027`) and ten horizontal centers (`51, 137, 222, 307, 393, 478, 563, 648, 734, 819`) on the 1046×830 canvas. The user reports that two lines per axis still look missing; confirm the page and positions before further geometry changes.

## Resolving the missing displayed grid lines

- Inspected the user's 446×351 screenshot. It showed two missing interior lines per axis even though the earlier browser check found all ten lines in the 1046×830 backing canvas. The display is downscaled with pixelated rendering, so the narrow strokes were dropped in the displayed output.
- Thickened each interior grid line with three adjacent BASIC strokes and updated the regression to inspect a nearest-neighbor copy at the CSS display size, rather than only checking backing pixels. Updated the WASM guide and handoff to record the rendering constraint and corrected check.
- Validation passed: the focused browser test passed (1/1, 2.8 minutes); `node --check` passed for both changed JavaScript files; `git diff --check` passed; and the updated WASM Typst guide compiled. The screenshot remains the user's untracked reference file at `dev/images/202611202108/file.png`.

## Starting release WASM with Extended WebGPU

- The existing port 8000 server was unavailable, so started the local Python static server and verified its browser-shell response.
- Opened the release WASM page at `?renderer=extended&build=20261120-2221`, selected Extended WebGPU, and started the emulator. It reached the BASIC prompt and is running at x1 with POWER on.
- Current browser page ID: `e0a5fdae-bb4b-4885-a6ba-081b1b2975c7`. The local server is left running so the emulator page remains usable.

## Correcting WASM canvas downscaling

- The user confirmed the thickened grid looked correct, then clarified that downscaling must preserve thin lines rather than rely on thicker strokes. The test program now draws one line per float-valued coordinate; browser canvases use filtered `image-rendering: auto`, allowing subpixel lines to appear thinner or dimmer instead of being skipped.
- The regression checks the CSS rendering mode and a smoothed copy at the displayed size. The focused BASIC browser test passed (1/1, 2.1 minutes). A separate Release WASM page using Extended WebGPU completed the grid test, returned to the BASIC prompt, and showed all ten line bands in its screenshot.
- The machine/emulation display pipeline still represents hardware raster positions as integers; the float scale is used in BASIC expressions, and fractional display scaling is handled by the browser. No machine-coordinate semantics were changed.

## Adding mouse camera controls

- The user requested wheel zoom and left-button drag panning, then specified pointer-centered zoom, a minimum of 0.25× fitted size, and right-click reset. Implemented an 8× maximum, float-valued CSS transforms, pointer capture during dragging, and help text.
- Added browser tests for pointer-anchored zoom, fractional pan offsets, right-click reset, and both zoom limits. The focused camera, keyboard, and layout tests passed (23/23, 4.9 seconds).
- Started a fresh Release WASM page with Extended WebGPU and verified the camera in the integrated browser: zoomed around the pointer, panned by fractional CSS offsets, and restored the original fitted bounds with right-click. The emulator remains running at x1 with the BASIC prompt detected.
- After camera integration, reran the BASIC display test (1/1, 2.1 minutes); the grid, snake fill, and circles still complete successfully. JavaScript syntax, `git diff --check`, and the WASM Typst guide compilation passed.

## Discussing high-definition zoom

- The user asked whether a separate zoom mode could redraw vectors at the selected scale while retaining the current bitmap zoom for inspecting raster quality. No code was changed.
- The current browser camera transforms the fixed-resolution canvas. `VectorSegment` already carries float endpoints and width in the WebGPU renderer, but machine display points are integer raster positions; the Extended WebGPU phosphor stamp is also a fixed-size per-pixel texture, and pending segment geometry is cleared after stamping.
- High-definition redraw is feasible by retaining vector scene data and rasterizing it at the visible output resolution. Extended WebGPU would also need enough per-segment timing/coverage history to reconstruct glow; the CPU reference path currently has only raster pixels unless its model is extended. Decide renderer coverage and the desired Extended glow behavior before implementation.

## Refining high-definition zoom semantics

- The user clarified that zoom is a virtual camera change, not a machine drawing operation. Persistent vector detail should be rerendered at normal steady-state intensity; an expired glow must not be recreated. Current live drawing/cursor glow may remain as a separate overlay if feasible, with its existing age continuing through camera changes.
- This separates persistent vector scene data from transient glow state: camera changes rerasterize the base scene only, never replay BASIC commands or refresh phosphor timestamps. No implementation code changed; renderer coverage and live-overlay quality remain design choices.

## Discussing modular high-definition zoom

- The user asked that high-definition zoom be isolated from existing rendering, with an on/off toggle. Proposed a separate presentation module and optional renderer path; the existing bitmap camera remains the default and is unchanged when high-definition mode is off.
- To make already-drawn content available immediately when toggled on, a sidecar must keep a vector scene synchronized from display events while not rendering it. This adds bounded event/memory overhead while off; avoiding that overhead means only future drawing can be vector-redrawn. No implementation code changed.

## Defining high-definition zoom scope

- The user specified that CPU fallback remains bitmap-only; Extended WebGPU is the first target, and normal WebGPU may adopt the feature in a later iteration. Shared vector-scene/camera logic should remain separate from Extended WebGPU where practical, complexity should stay low, and performance must be benchmarked.
- All scene information must be captured continuously, independent of the toggle; raster frames are derived views of the vector scene. The presentation camera does not replay machine drawing or alter the BASIC sequence.
- High-definition mode remains optional and existing bitmap rendering remains the default. No implementation code changed.

## Correcting WASM canvas downscaling

- The user confirmed the thickened grid looked correct, then clarified that downscaling must preserve every thin line rather than rely on thicker strokes. The test program now draws one line per float-valued coordinate; browser canvases use filtered `image-rendering: auto`, allowing subpixel lines to appear thinner or dimmer instead of being skipped.
- The regression now checks the CSS rendering mode and a smoothed copy at the displayed size. The focused BASIC browser test passed (1/1, 2.1 minutes). A separate Release WASM page using Extended WebGPU completed the grid test, returned to the BASIC prompt, and showed all ten line bands in its screenshot.
- The machine/emulation display pipeline still represents hardware raster positions as integers; the float scale is used in BASIC expressions, and fractional display scaling is handled by the browser. No machine-coordinate semantics were changed.
- The user also requested mouse-wheel zoom and left-button drag panning for the display, with right-click resetting the camera. Zoom anchoring and the minimum zoom remain to be confirmed.

## Auditing high-definition vector zoom requirements

- Compared the agreed vector-camera behavior with the WebGPU renderer plan and WASM guide. The scope is coherent as an optional Extended-WebGPU-first presentation path with continuous renderer-independent scene capture, existing bitmap presentation as the default, CPU fallback bitmap-only, and no BASIC replay or glow-age reset during camera movement.
- Identified a plan-scope conflict: the renderer plan says no new renderer-settings UI, while the later approved vector-zoom request requires an on/off toggle. The newer request supersedes that earlier scope, but the plan should be reconciled before implementation.
- Recorded unresolved acceptance details: whether CPU bitmap-camera zoom remains available (it currently is), whether machine screen-copy/API captures follow the camera, which display operations and lifecycle events the retained scene covers, bounded memory/overflow behavior, the minimum live-glow overlay behavior, and benchmark workloads/thresholds. No implementation code changed; resolve these points before implementation.

## Reviewing vector-zoom clarifications

- The user confirmed that CPU fallback lacks vector-quality zoom only; the existing bitmap camera remains available. UI screenshots capture the current camera view.
- The retained scene must represent all supported visible content. Memory requirements should be measured from representative test scenes and reported; live glow must be preserved. Performance will be measured after implementation, with the decision to proceed based on the results rather than a preselected threshold. The vector-zoom rendering path must be disableable as an add-on to existing behavior.
- Review note: keep UI screenshots distinct from machine ScreenCopy/API snapshots until their camera-transform behavior is specified. Also distinguish disabling vector rendering from disabling continuous scene recording: recording remains necessary for immediate activation over already-drawn content, so turning off only the render mode does not remove its overhead. No implementation code changed.
