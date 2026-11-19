#set par(justify: true)
#set page(
  margin: 40mm,
  header-ascent: 18pt,
  footer-descent: 18pt,
  header: context {
    if counter(page).get().first() > 1 {
      let prior = query(heading.where(level: 1, outlined: true))
        .filter(h => h.location().page() <= here().page())
      let label = if prior.len() > 0 { prior.last().body } else { [] }
      grid(
        columns: (1fr, auto, 1fr),
        gutter: 0pt,
        align: (left, center, right),
        [],
        label,
        [],
      )
    }
  },
  footer: context {
    let page_no = counter(page).display("1")
    if counter(page).get().first() == 1 {
      align(center)[#page_no]
    } else {
      align(right)[#page_no]
    }
  },
)
#set text(font: "Arial", size: 10pt)
#set heading(numbering: "1.1")

#align(center)[
  #text(size: 20pt, weight: "bold")[Tektronix 4051 Emulator]
  \
  #text(size: 15pt)[Optional WASM Vector Zoom Implementation Plan]
]

#align(center)[
  Track: high-definition, camera-driven vector presentation
  \
  Boundary: Extended WebGPU presentation; Machine and machine-copy semantics unchanged
  \
  Status: implementation in progress; optional capture, camera-driven Extended WebGPU vector presentation, Raster/Vector selector, and focused renderer tests implemented; correctness-gated long-BASIC speed/resource benchmarks, browser WebGPU acceptance, and matched measurements pending
  \
  Updated: 21 November 2026
]

#outline(title: [Contents], depth: 2)

= Purpose and planning basis

This plan adds an optional vector-rendered camera mode to the existing WASM Extended WebGPU presentation. The current bitmap camera remains the default path. The vector mode re-rasterizes retained display geometry for the current camera view; it does not replay BASIC commands or change the machine's drawing sequence.

The clarified conversation is the canonical feature requirement source. The existing renderer plan and inspected source define the implementation boundaries. No standalone feature-spec or project constitution file was found. This plan is a follow-on to the #link("./graded-wasm-webgpu-renderer-plan.typ")[graded WebGPU renderer plan], not a replacement for its renderer/capture acceptance work.

= Scope and frozen decisions

- The feature is initially available only when Extended WebGPU is active. The CPU fallback retains its existing bitmap zoom; normal WebGPU support is deferred.
- A master feature toggle belongs on the Test tab and starts off. When off, do not allocate or retain vector-scene state, process scene-recording events, or render the vector scene. Keep existing raster zoom behavior.
- Enabling the master toggle is an explicit hard cut: call the existing reset path and clearly tell the user that the emulator resets. Arm scene capture at this boundary, before post-reset display operations arrive. Do not change Machine reset behavior.
- While the master feature is enabled, a main-menu selector switches between Raster and Vector views. Scene recording continues in Raster mode so switching to Vector shows the scene captured since the hard cut. Turning the master toggle off stops and releases vector-only work.
- The selector starts in Raster mode when the master toggle is enabled. This preserves current presentation until the user chooses Vector. The existing camera zoom/pan state is shared between modes; switching modes does not reset the camera.
- Capture all supported visible display content in the emulation/rendering layer above `Machine`. Keep camera state out of Machine, machine `ScreenCopy`, and the `BrowserSnapshot` pixel/snapshot contract.
- UI/browser screenshots show the current camera view. Machine `ScreenCopy` and snapshot APIs remain camera-independent and preserve their current dimensions and semantics.
- Persistent geometry is rendered at normal steady-state intensity. Preserve live glow and its existing age through camera and Raster/Vector changes; never restamp expired glow because the retained scene is re-rendered.
- Measure scene memory and performance after implementation. Report observations without a preselected pass threshold; the user decides whether and how to proceed.
- Long-BASIC performance runs must use programs that have first passed correctness preflight. The corpus includes selected supplied Flash Drive programs and deterministic generated stress programs. Do not treat key-entry time or prompt-detection delay as CPU/render performance.

= Responsibility boundaries

#table(
  columns: (1.3fr, 2.8fr, 2.6fr),
  stroke: 0.5pt,
  [*Area*], [*Existing responsibility / evidence*], [*Plan boundary*],
  [Machine], [#link("../../../../src/4051-Emulator/rs/4051-Emulator-rs/machine/display.rs")[`machine/display.rs`] emits ordered display operations, including screen-copy requests.], [Do not add camera state, vector-zoom controls, or new behavior to Machine or its operation contract.],
  [Emulation display], [#link("../../../../src/4051-Emulator/rs/4051-Emulator-rs/emulation/runtime.rs")[`emulation/runtime.rs`] consumes ordered operations and renderer-derived display events. #link("../../../../src/4051-Emulator/rs/4051-Emulator-rs/emulation/extended_webgpu.rs")[`extended_webgpu.rs`] owns baseline composition and transient phosphor stamps.], [Add an optional renderer-independent scene recorder above Machine. Keep it inactive when the master toggle is off and keep its camera transform separate from screen-copy capture.],
  [WASM presentation], [#link("../../../../src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/mod.rs")[`platforms/wasm/mod.rs`] exposes browser session and WebGPU setup. #link("../../../../src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser/app.js")[`browser/app.js`] owns browser UI bindings.], [Connect the Test-tab master toggle and main-menu mode selector to presentation-only rendering controls. Do not make snapshots or ScreenCopy camera-aware.],
  [Camera], [#link("../../../../src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser/display_camera.js")[`display_camera.js`] currently keeps floating zoom/pan state and applies CSS transforms to the display canvases.], [Keep one camera state. Raster mode uses the existing CSS transform; Vector mode applies an equivalent scene-to-view transform in rendering and must not also apply the CSS zoom a second time.],
  [Tests], [#link("../../../../src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser-tests/display-camera.spec.mjs")[`display-camera.spec.mjs`] covers current pointer-anchored zoom, pan, reset, and bounds.], [Extend focused tests for mode switching, reset-on-enable, camera-aware UI screenshots, and unchanged machine ScreenCopy/snapshot behavior.],
)

= Requirements

#table(
  columns: (0.9fr, 3.4fr, 1.8fr),
  stroke: 0.5pt,
  [*ID*], [*Required outcome*], [*Planned evidence*],
  [REQ-001], [Master toggle is Test-tab-only, initially off, and unavailable when Extended WebGPU is not active. Off preserves the existing raster camera and performs no vector-scene allocation, recording, or rendering.], [Stages 1 and 5; off-path behavior and measurements.],
  [REQ-002], [Enabling the master toggle clearly warns that it resets the emulator and invokes the existing reset path as a hard cut. Scene capture starts at that reset boundary.], [Stage 1; reset lifecycle tests.],
  [REQ-003], [While enabled, retain all supported visible display content in a renderer-independent scene above Machine; report measured scene memory for the test corpus and surface failures rather than silently dropping content.], [Stages 2 and 5; event coverage and memory report.],
  [REQ-004], [A main-menu selector, visible only while enabled, switches between Raster and Vector views. The shared floating camera supports the existing 0.25×–8× wheel zoom, pointer anchoring, pan, and reset.], [Stages 3 and 4; browser camera/mode tests.],
  [REQ-005], [Vector mode renders the retained scene at the selected camera view without replaying BASIC or changing machine/display operations. All vector strokes, including those that also emit transient-glow events, retain normal steady-state intensity; camera state survives mode switching.], [Stages 3 and 5; scene/render equivalence tests.],
  [REQ-006], [Live glow continues to age normally across camera and mode changes. Re-rendering persistent scene geometry does not recreate or refresh expired glow.], [Stage 4; age and expiry tests.],
  [REQ-007], [Browser/UI screenshots show the camera view. Machine ScreenCopy and snapshots remain camera-independent, with existing ordering, dimensions, and pixel contracts.], [Stages 3 and 5; public API and browser checks.],
  [REQ-008], [Vector zoom targets Extended WebGPU first. CPU fallback stays bitmap-only; normal WebGPU vector zoom is deferred.], [Stages 1 and 5; renderer capability/status tests.],
  [REQ-009], [After implementation, report memory and performance for feature-off, enabled/Raster, and enabled/Vector modes on matched, correctness-verified long BASIC workloads at x1, x2, x5, x10, x20, x50, and x100; make no keep/reject decision until the user reviews the data.], [Stage 6; reproducible measurement report.],
  [REQ-010], [Benchmark only supplied or generated BASIC programs that pass a correctness preflight; verify execution start/completion state, error state, and expected observable output before collecting timing or resource data. Record and exclude failing supplied programs.], [Stages 5 and 6; preflight evidence and excluded-workload record.],
)

= Staged implementation

== Stage 0 — Reconcile contracts and establish controls

- #raw("[ ] T001") Update the existing renderer plan and design index to identify vector zoom as a separately authorized follow-on. Clarify that renderer UI is excluded from the original renderer grades but the new Test-tab and main-menu controls are in this feature. Preserve the active-renderer capture requirement while explicitly excluding camera transforms from machine ScreenCopy/snapshots. [REQ-007, REQ-008]
- #raw("[ ] T002") Freeze the scene event/lifecycle matrix from the current `DisplayEvent` and `DisplayOperation` path: stored/transient vectors and marks, granularity, display commands, refresh, erase begin/completion, reset, and ordered screen-copy requests. Define which events change retained scene state and which remain capture-only. [REQ-003, REQ-005, REQ-007]
- #raw("[ ] T003") Record the current Raster camera/test-scene behavior and a matched release-build baseline before changing code. Use only correctness-verified scenes; keep the existing short Grid, Snake fill, and Concentric circles checks distinct from the long-BASIC benchmark corpus. The baseline is for later comparison, not a performance decision. [REQ-001, REQ-004, REQ-009, REQ-010]

*Stage exit:* the UI/reset contract, scene coverage, baseline workloads, and Machine/ScreenCopy boundary are documented.

== Stage 1 — Add the master feature gate and hard-cut lifecycle

- #raw("[x] T004") Add the initially-off master toggle to the Test panel in `platforms/wasm/browser/index.html`; label its reset side effect. Keep it disabled unless the active renderer is Extended WebGPU. [REQ-001, REQ-002, REQ-008]
- #raw("[x] T005") Bind the toggle in `platforms/wasm/browser/app.js`. On enable, initialize the optional scene recorder, invoke the existing `emulator.reset()` path, and begin collecting from the reset boundary. On disable, stop capture and release vector-only scene/render resources without resetting the machine. Surface failures in existing status/error UI. [REQ-001, REQ-002, REQ-003]
- #raw("[x] T006") Add the main-menu Raster/Vector selector, hidden while the master toggle is off and initialized to Raster when enabled. Preserve the current camera state through toggles. [REQ-001, REQ-004]
- #raw("[ ] T007") Add focused browser tests proving the feature-off path keeps existing raster camera behavior, allocates no vector scene, does not expose the mode selector, and leaves snapshots/ScreenCopy behavior unchanged. Verify that enabling triggers exactly the existing reset path. [REQ-001, REQ-002, REQ-007]

*Stage exit:* feature-off behavior remains the current browser behavior; enabling starts a fresh, explicit capture session.

== Stage 2 — Record a renderer-independent scene

- #raw("[x] T008") Add a small renderer-independent scene representation under `emulation/` (candidate: `emulation/vector_scene.rs`). Store the geometry and semantic state required to reproduce every supported visible display element; do not put camera state in the scene. [REQ-003, REQ-005]
- #raw("[x] T009") Tee ordered display events into the optional recorder from the existing runtime/emulation rendering boundary. Do not alter Machine operations, `ScreenCopy`, or `BrowserSnapshot`. Keep the recorder absent and avoid event cloning/scene work when the master feature is off. [REQ-001, REQ-003, REQ-007]
- #raw("[x] T010") Define erase, refresh, granularity, reset, cursor-mark, and vector-glow lifecycle behavior in the scene. Refresh removes transient cursor marks but retains all vector strokes; transient glow expires independently of its retained base geometry. `PAGE`/erase and the master-toggle hard cut must not leave stale vector geometry; screen-copy requests must not apply camera transforms or mutate the scene. [REQ-002, REQ-003, REQ-007]
- #raw("[x] T011") Instrument scene entry counts and application-owned retained bytes/high-water usage. Do not impose an arbitrary cap based on guesses; report measured memory for the agreed test scenes and surface allocation/renderer failures without silently discarding geometry. [REQ-003, REQ-009]
- #raw("[x] T012") Add Rust tests for every supported event category, ordering, erase/reset boundaries, feature-off inactivity, and scene memory accounting. Keep tests in the package's existing separate test files. [REQ-001, REQ-002, REQ-003]

*Stage exit:* the scene represents the complete visible test corpus and can be disabled/released without affecting machine behavior.

== Stage 3 — Render vector geometry through the shared camera

- #raw("[x] T013") Extend the existing camera binding to expose one floating-point view transform in scene/output coordinates. Keep Raster mode's CSS transform unchanged; in Vector mode apply the equivalent transform in the Extended WebGPU rendering path and avoid double-applying CSS zoom. [REQ-004, REQ-005]
- The camera binding reports zoom and logical-display pan, updates Extended WebGPU without advancing emulation, and suppresses only the WebGPU canvas CSS transform in Vector mode.
- #raw("[x] T014") Render the retained scene at the current camera view using Extended WebGPU and the active output size. Keep coordinates and widths in floating-point presentation math where appropriate; do not change machine raster coordinates or operation semantics. [REQ-003, REQ-004, REQ-005]
- #raw("[ ] T015") Implement Raster/Vector switching without re-running BASIC or altering scene history. Confirm camera zoom, pan, pointer anchor, and right-click reset behave consistently in both modes. Keep the CPU fallback bitmap-only. [REQ-004, REQ-005, REQ-008]
- The selector, mode APIs, and camera-preserving renderer transitions are implemented. The end-to-end browser confirmation of pointer anchoring/reset while Vector mode is active remains part of T019 because the available browser cannot initialize WebGPU.
- #raw("[ ] T016") Verify UI/browser screenshots show the selected camera view while machine ScreenCopy and snapshot outputs retain their existing framing, ordering, dimensions, and pixel source. [REQ-007]
- Native GPU tests verify camera rerasterization, age-preserving glow, and camera-independent ScreenCopy pixels. Browser mode-switch/screenshot acceptance remains pending because the available browser does not initialize WebGPU.

*Stage exit:* Vector mode shows the same captured scene through the current camera without replaying emulation events or changing machine capture behavior.

== Stage 4 — Preserve live glow and lifecycle behavior

- #raw("[x] T017") Keep persistent geometry and transient live glow as separate presentation inputs. Preserve current live glow age and decay through camera movement and Raster/Vector changes; do not restamp the scene when the camera moves. [REQ-005, REQ-006]
- #raw("[ ] T018") Test active glow, cursor/transient marks, expiry, erase, repeated mode switches, master disable/re-enable, and reset. Confirm expired glow never reappears after a vector rerender and the existing Extended WebGPU behavior is unchanged when the feature is off. [REQ-002, REQ-006]

*Stage exit:* live glow remains age-correct; expired glow is never recreated by scene rendering.

== Stage 5 — Integrated browser acceptance

- #raw("[ ] T019") Extend `platforms/wasm/browser-tests/` coverage for feature availability, the reset warning/hard cut, scene capture of the BASIC test programs, mode switching, camera-state continuity, glow lifecycle, screenshot framing, and ScreenCopy/snapshot isolation. Correctness-check each candidate long BASIC workload separately: require successful program completion or its documented termination condition, no runtime/BASIC error, and expected display/scene output before admitting it to benchmarks. Supplied Flash Drive programs that fail this gate are recorded and excluded rather than treated as required passing tests. [REQ-001–REQ-008, REQ-010]
- #raw("[ ] T020") Run the focused Rust scene/renderer tests, WASM API tests, and Playwright display-camera/BASIC-display tests. Build the release WASM with Extended WebGPU and confirm CPU fallback still operates with vector zoom unavailable. [REQ-001–REQ-008]

*Stage exit:* the opt-in browser feature passes functional checks and the existing feature-off path remains unchanged.

== Stage 6 — Measure and report; defer the product decision

- #raw("[ ] T021") Build the long-BASIC corpus from correctness-verified supplied Flash Drive programs and deterministic generated stress programs. Record each source, setup, expected observable output, execution-start signal, termination condition or fixed simulated workload window, and preflight evidence. Record and exclude supplied programs that fail preflight. Keep program setup and sequenced keyboard-entry time outside the timed interval; start timing only after program execution is confirmed. The current Concentric circles browser program is not eligible until its zero/low-pixel failure is resolved. [REQ-009, REQ-010]
- #raw("[ ] T022") Extend benchmark telemetry to report retained-scene event counts/current/peak bytes, vector-buffer capacities, GPU buffer sizes/capacity high-water marks, and texture dimensions/estimated bytes. Distinguish exact application-owned sizes from estimates; explicitly mark driver-only allocation totals or browser memory metrics as unavailable when they cannot be measured. [REQ-003, REQ-009]
- #raw("[ ] T023") For each admitted program, measure the same Release Extended-WebGPU build in three states—feature off, feature on/Raster, and feature on/Vector—at every CPU speed x1, x2, x5, x10, x20, x50, and x100. Reset to the same initial state for each sample; capture warm-up and repeated runs, and verify expected completion/output again so a failed run cannot appear as a fast result. Include the agreed scene densities and camera levels through 8×. Report elapsed program time, emulation/workload utilization, and frame-time distribution. [REQ-009, REQ-010]
- #raw("[ ] T024") Publish the full program × mode × speed results, environment and adapter details, sample method, correctness evidence, exclusions, and measurement limits. Do not invent a numeric pass threshold or decide whether to retain the feature; present evidence for the user's decision. [REQ-009, REQ-010]

*Stage exit:* a reproducible report is reviewed by the user before any decision to keep, optimize, or disable the feature.

= Validation commands

From the repository root, use the existing WASM API and Playwright test commands as focused acceptance checks:

```sh
cargo test --locked -p tek4051-emulator --no-default-features \
  --features wasm --test wasm_api

./node_modules/.bin/playwright test \
  --config dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser-tests/playwright.config.mjs \
  dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser-tests/display-camera.spec.mjs \
  dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser-tests/basic-display-tests.spec.mjs
```

Add focused Rust tests for the new renderer-independent scene and Extended WebGPU integration. Run the package formatter and the relevant native/wasm WebGPU checks when implementation begins. Compile this plan with `typst compile`; keep the generated PDF out of the repository.

= Requirement mapping

#table(
  columns: (0.8fr, 3.2fr, 1.9fr),
  stroke: 0.5pt,
  [*Requirement*], [*Implementation evidence*], [*Plan tasks*],
  [REQ-001], [Test-tab master toggle; no vector allocations/recording/rendering while off; unchanged Raster path.], [T004–T007, T009, T012, T018–T020],
  [REQ-002], [Existing reset invoked as explicit hard cut; capture starts at boundary; scene lifecycle clears correctly.], [T005, T007, T010, T012, T018–T020],
  [REQ-003], [Renderer-independent scene, full display-event coverage, memory/high-water reporting, visible failures.], [T002, T008–T012, T014, T019–T023],
  [REQ-004], [Main-menu Raster/Vector selector and shared camera behavior.], [T006, T013–T015, T019],
  [REQ-005], [Vector rerender from retained scene; persistent steady intensity; no emulation replay.], [T008–T010, T014–T018, T019],
  [REQ-006], [Live glow age is preserved; expired glow is not recreated.], [T017–T019],
  [REQ-007], [UI screenshots follow camera; Machine ScreenCopy and snapshots remain unchanged.], [T002, T007, T009–T010, T016, T019],
  [REQ-008], [Extended-WebGPU-only vector mode; CPU fallback remains bitmap-only; normal WebGPU deferred.], [T004, T015, T019–T020],
  [REQ-009], [Matched three-state memory/performance report at all seven CPU speeds on correctness-verified long BASIC workloads, with no preselected acceptance threshold.], [T003, T011, T021–T024],
  [REQ-010], [Preflight evidence confirms each benchmark program's execution and expected output; invalid or unresolved programs, including failing supplied Flash Drive programs, are excluded and recorded.], [T019, T021, T023–T024],
)

= Assumptions and deferred scope

- The master toggle is session-local and defaults off; no persistence is added.
- Enabling the master toggle starts the feature in Raster mode; Vector mode requires an explicit selection.
- When the master toggle is off, the recorder does not run. Re-enabling always resets and begins a new capture, so no scene reconstruction from pre-enable history is required.
- No arbitrary scene-memory cap is selected in advance. Use measured test-scene memory and report longer-running growth before proposing a bound.
- The long-BASIC benchmark corpus includes both supplied Flash Drive programs and deterministic generated stress programs, but only after each candidate passes the correctness gate. Measure emulator/render workload rather than BASIC line-entry or readiness-detection latency.
- Long-running programs need a verified start signal and a fixed simulated-work window; finite programs need a verified termination condition. Do not use prompt readiness alone as proof that a benchmark workload executed correctly.
- Report application-owned CPU scene storage and GPU allocation sizes/capacity where observable. Mark estimates explicitly; do not present texture-size arithmetic or browser-reported heap availability as exact total device memory.
- Normal WebGPU vector zoom, CPU vector zoom, new machine screenshot/export APIs, camera-aware machine ScreenCopy, ROM/BASIC changes, and persistence are out of scope.
- The existing renderer plan's quality gate against claiming vector-quality improvement from resolution scaling alone remains distinct from this feature's user-experience goal: compare Raster and Vector at the same camera view to demonstrate the added rerasterization detail, while report resolution scaling as a separate usability benefit.
