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
  #text(size: 15pt)[Graded WebGPU Renderer Implementation Plan]
]

#align(center)[
  Track: higher-quality vector rendering for the Rust WASM emulator
  \
  Starting boundary: machine display operations consumed by emulation
  \
  Status: WASM WebGPU presentation implemented; release runtime confirmed; active-renderer capture and final acceptance remain open
  \
  Updated: 19 November 2026
]

#outline(title: [Contents], depth: 2)

= Purpose and scope

This plan adds a WebGPU vector renderer to the Rust emulator, with the native Rust implementation and validation completed before browser integration. WebGPU is preferred in browsers that can initialize it; the existing CPU reference renderer remains available as a fallback and behavioral reference.

The WebGPU renderer is intended to improve the visual quality of vector graphics. Its pixels are expected to differ from the CPU reference, so pixel equality is not a compatibility gate. Compatibility means preserving implemented machine/display behavior, display-operation ordering, user controls, readiness signals, erase behavior, and screen-copy outcomes. Do not change emulator features, BASIC behavior, supplied ROM/program assets, or machine semantics as part of this work.

The machine remains renderer-agnostic. It emits the existing ordered `DisplayOperation` stream; renderer ownership and display interpretation stay above `Machine` in emulation. Browser canvas, adapter/device, and surface lifecycle belong at the WASM host boundary. The GPU renderer must consume vector/display operations rather than upload and enlarge the CPU framebuffer.

== Frozen decisions and scope limits

- Browser rendering prefers WebGPU when the API, adapter, and device are available. The CPU reference renderer remains the fallback.
- The CPU reference feature set remains intact. WebGPU may intentionally differ in anti-aliasing, line coverage, and other pixels needed for improved vector quality.
- Captures and machine screen-copy requests must reflect the active renderer, not silently use the CPU reference while WebGPU is displayed.
- The first implementation and acceptance increments run in the Rust emulator/native environment. Browser/WASM integration starts only after the native Rust renderer gate passes.
- The CPU reference renderer remains independently selectable for comparison, fallback, and regression diagnosis.
- This plan does not require replacing the native egui presentation backend. Native Rust work may use a feature-gated offscreen renderer and readback harness to prove the shared Rust renderer before browser integration.
- The original renderer grades add no persistence, renderer settings UI, or screenshot feature. The later-approved optional vector zoom is a separate follow-on with temporary Test-tab and main-menu controls, specified in #link("./vector-zoom-implementation-plan.typ")[the vector zoom implementation plan]; it adds no screenshot/export API and does not change machine ScreenCopy semantics.

The user approved qualitative side-by-side visual comparison at matching output sizes; no numeric threshold is required. WG-0 must record a representative vector corpus, CPU baselines, and review outcomes. The comparison must demonstrate smoother vector edges or another user-approved quality improvement while preserving endpoints, line continuity, stable apparent width, and display controls. Do not claim improvement from resolution scaling alone or use CPU pixel identity as a substitute.

= Baseline and responsibility boundaries

#table(
  columns: (1.2fr, 2.4fr, 2.4fr),
  stroke: 0.5pt,
  [*Area*], [*Current responsibility*], [*Planned responsibility*],
  [Machine display output], [`machine/display.rs` records sampled controls, screen commands, erase begin/completion, and screen-copy requests in operation order.], [Keep the contract platform-neutral and unchanged unless an independently approved behavior fix is required.],
  [Emulation rendering], [`emulation/display.rs` implements `CpuReferenceRenderer`, pixel revision, granularity, display events, phosphor refresh, erase, and ordered CPU screen copies.], [Retain the CPU reference; share operation interpretation/readiness semantics where needed; add the GPU renderer above `Machine`.],
  [Session and snapshots], [`emulation/session.rs` collects ordered screen copies and publishes immutable snapshots with cached CPU pixels.], [Keep operation/capture order and bounded queue behavior; extend the frame/capture contract deliberately for GPU output and asynchronous readback.],
  [WASM presentation], [`platforms/wasm/browser/app.js` presents the emulation-owned WebGPU texture on a dedicated canvas when initialization succeeds; a separate 2D canvas remains the explicit CPU fallback. `BrowserSnapshot` exposes the active renderer, while its `pixels()` API remains CPU-reference output and a screen-copy request can trigger a reported fallback.], [Complete active-renderer snapshot and screen-copy capture, then validate browser quality, device-loss recovery, responsiveness, and the supported-browser matrix.],
)

The renderer must preserve the observable behavior of ordered `ScreenCommand`, `BeginErase`, `CompleteErase`, and `ScreenCopy` operations. It must also preserve fine, coarse, and mixed display granularity; clipping and coordinate orientation; stored and transient marks/vectors; refresh behavior; and readiness detection. The readiness signal must not depend on antialiased framebuffer pixel values.

The current CPU erase sequence presents full-bright green between erase begin and completion. The CPU refresh callback removes cursor-level pixels while retaining stored vectors and dots. These are behavior constraints to preserve, not mandates to reproduce the CPU renderer's exact rasterization algorithm.

== Proposed technology approach

The preferred path is a Rust `wgpu` backend using WGSL, first exercised with a native adapter and then with browser WebGPU. This offers a practical way to share renderer logic and shaders across the two stages. Grade WG-1 must verify the locked dependency/API versions, target support, feature flags, canvas integration, and native test strategy before committing to that path. A transitive lockfile entry is not by itself a reason to depend on a crate directly.

Keep the renderer behind a small emulation-owned contract. Avoid adding per-instruction dynamic dispatch, allocations, GPU readbacks, or browser dependencies to the machine execution path. Decide whether operation decoding and semantic display events should be factored out of `CpuReferenceRenderer` before duplicating any interpretation in the GPU backend. The GPU backend must not infer readiness by sampling its visual output.

= Grade overview

#table(
  columns: (0.6fr, 1.8fr, 2.6fr),
  stroke: 0.5pt,
  [*Grade*], [*Boundary gained*], [*Primary exit evidence*],
  [WG-0], [Frozen feature/quality contract], [Operation matrix, visual corpus, approved improvement criterion, and CPU baseline captures.],
  [WG-1], [Renderer and native proof contract], [Backend/API choice, lifecycle and capture design, plus a native offscreen proof.],
  [WG-2], [Rust GPU vector rasterizer], [Native renderer improves the agreed vector cases and passes operation/semantic tests.],
  [WG-3], [Integrated Rust emulation renderer], [Renderer selection, CPU reference fallback, snapshots, readiness, and screen-copy ordering work in Rust.],
  [WG-4], [Browser WebGPU presentation], [WASM prefers WebGPU when usable and clearly falls back to CPU otherwise.],
  [WG-5], [Active-renderer capture], [Snapshot and screen-copy outputs match the selected renderer with bounded, ordered readback.],
  [WG-6], [Browser quality and release acceptance], [Supported-browser, quality, failure, responsiveness, and regression evidence is recorded.],
)

Grades WG-0 through WG-3 are Rust emulator work. A failure in a native renderer grade blocks browser integration; a later browser grade does not waive an earlier exit gate.

= Grade WG-0 - Freeze feature and visual-quality contract

*Goal:* define what must remain behaviorally compatible and how vector-quality improvement will be judged before choosing or implementing a rasterization algorithm.

*Work:*

- Inventory each `DisplayOperation` and its ordering, including sampled display controls, alpha dots, stored/transient vectors, erase phases, refresh, and screen-copy requests.
- Record current CPU behavior for fine, coarse, and mixed granularity, coordinate mapping, clipping, dot/vector placement, revision changes, readiness events, and screen-copy ordering.
- Create a deterministic display-operation corpus containing horizontal, vertical, diagonal, shallow-slope, joined, clipped, and zero-length vectors; stored/transient marks; all granularity modes; refresh; erase; and capture points.
- Save CPU reference output for each corpus case as a behavioral baseline. Label which properties are semantic invariants and which pixel details may improve or differ.
- Compare CPU and GPU output side by side at matching output sizes for the agreed vector cases. Qualitative visual judgment is the accepted method; no numeric threshold is required. The review must demonstrate smoother vector edges or another user-approved quality improvement while preserving endpoints, line continuity, stable apparent width, and display controls. Record the review outcomes and do not claim improvement from resolution scaling alone.
- Confirm that emulation readiness and transient cursor detection remain based on display semantics, not GPU edge pixels.

*Exit gate:* the operation/feature matrix, corpus, baseline outputs, and observable quality-improvement criterion are recorded. The user-approved criterion explicitly permits GPU pixel differences and does not relax feature compatibility.

= Grade WG-1 - Establish the renderer contract and native proof

*Goal:* prove that the selected Rust GPU API can render the emulator's display operations offscreen before changing the native application or WASM shell.

*Work:*

- Verify `wgpu`/WGSL or document an alternative using the current Rust toolchain, locked dependency graph, native target, and `wasm32-unknown-unknown`. Decide feature gating and avoid enabling GPU dependencies in builds that do not request the backend.
- Define the emulation-facing renderer contract: dimensions, coordinate system, green-channel/intensity representation, granularity, operation ordering, revisions, initialization errors, and explicit backend identity.
- Define how shared operation interpretation supplies both renderer geometry and readiness events without coupling readiness to raster pixels.
- Define renderer lifecycle ownership: emulation owns display behavior; native/browser adapters supply platform resources and presentation hooks. The machine receives no renderer/device types.
- Build a minimal native offscreen proof that creates the GPU backend, renders a known line and dot, reads back a test image, and reports unavailable adapter/device errors explicitly.
- Decide the browser canvas/context fallback strategy before acquiring either context. A failed WebGPU attempt must not leave the CPU 2D fallback unusable because the same canvas was already locked to another context type.
- Specify how GPU `ScreenCopy` requests snapshot the exact ordered frame and how asynchronous readback, completion, failures, queue bounds, and notices preserve current semantics.

*Exit gate:* the native offscreen proof passes on an available adapter and reports a clear, tested failure when no adapter/device is available. The renderer/capture contract and dependency feature matrix are documented; no machine or emulator feature behavior has changed.

= Grade WG-2 - Implement the Rust GPU vector rasterizer

*Goal:* implement the GPU renderer against the frozen operation corpus and demonstrate the agreed visual improvement in native Rust.

*Work:*

- Implement GPU geometry/coverage for vectors and marks with antialiased edges. Choose analytic coverage, supersampling, or another documented method using corpus quality and performance evidence rather than assumption.
- Preserve operation order, logical endpoints, coordinate orientation, clipping, stored/transient distinctions, dot sizing, and fine/coarse/mixed granularity.
- Implement persistent display state and the required refresh and erase transitions without relying on CPU-rendered pixels as the GPU input.
- Keep semantic display events and BASIC readiness behavior equivalent to the shared emulation contract, including when GPU initialization fails or the CPU renderer is selected.
- Add deterministic renderer tests for operation order, endpoints, clipping, granularity, erase/refresh transitions, revision behavior, and semantic readiness inputs. Keep private tests in separate private test modules and public API tests under the package `tests/` directory.
- Compare the native GPU images to the WG-0 corpus for visual acceptance; compare semantic invariants separately and do not require pixel equality with CPU output.

*Exit gate:* the native GPU renderer passes the frozen semantic and boundary cases, improves the approved vector-quality cases, reports initialization/render failures, and does not change CPU reference results or machine behavior.

= Grade WG-3 - Integrate and validate in the Rust emulator

*Goal:* make the GPU renderer a usable emulation-owned backend and complete the native-first acceptance before browser work begins.

*Work:*

- Integrate renderer selection above `Machine` in the emulation/session boundary. Keep `Machine` limited to ordered display operations and retain the CPU reference as a selectable backend and fallback.
- Ensure session snapshots, revisions, refresh, granularity changes, erase timing, display events, and screen-copy requests observe the selected renderer contract.
- Provide a native test/preview path that exercises the real Rust session and operation stream. Use offscreen readback for validation; do not replace the egui renderer backend unless a separate measured need and design decision require it.
- Test GPU-to-CPU fallback and CPU reference selection, including clear status/error reporting and cleanup of partially initialized GPU resources.
- Run the existing emulator tests, no-default-feature library tests, native build, formatter, and strict Clippy. Add focused regression tests for only the new backend contract and behavior.
- Review the emulation/display ownership and changed tests with the required Rust Code Quality Reviewer and Rust Test Quality Reviewer; resolve actionable findings before proceeding.

*Exit gate:* the native Rust session runs the corpus through the GPU renderer and the CPU reference remains usable. Existing implemented emulator features and readiness behavior pass regressions. Native tests/build/lint and both focused Rust reviews have no unresolved actionable findings. If native GPU execution is unavailable in the environment, record the blocked evidence and do not mark the grade passed.

= Grade WG-4 - Add browser WebGPU selection and presentation

*Goal:* connect the Rust GPU renderer to the WASM browser shell while preserving a robust CPU fallback.

*Work:*

- Add the target-specific WASM GPU dependencies/features and required browser bindings only after WG-1 confirmed the dependency and API choices.
- Integrate device/adapter and canvas presentation lifecycle at `platforms/wasm`; keep the renderer implementation and display semantics in Rust emulation.
- Probe for WebGPU and attempt initialization before locking the display canvas to a rendering context. Select WebGPU when usable; otherwise select the CPU reference renderer and show the selected backend and fallback reason in runtime status.
- Make the browser shell present WebGPU output directly instead of uploading the CPU bitmap to the GPU. Keep the existing CPU pixel/2D-canvas path functional as fallback.
- Handle resize, unsupported features, adapter/device creation failure, device loss, and shutdown explicitly. Recover where supported; otherwise switch to CPU with a visible status and no lost emulator state.
- Keep the animation loop revision-aware. Do not copy or read back the full GPU framebuffer on every frame merely to update controls.
- Add focused browser tests for supported WebGPU, unsupported WebGPU, initialization failure, fallback usability, selected-backend reporting, and keyboard/emulator operation while either renderer is active.

*Exit gate:* in supported browsers the renderer is WebGPU-backed; in unsupported or failed initialization cases the CPU reference remains usable and the reason is visible. The same Rust session and controls work in both modes, with no GPU dependency in `Machine`.

= Grade WG-5 - Match captures to the active renderer

*Goal:* ensure snapshots and machine screen-copy requests capture the image actually being presented, without per-frame GPU readback.

*Work:*

- Define which browser snapshot/capture APIs need synchronous pixels and which can complete asynchronously. Preserve existing public behavior when possible; if GPU readback requires an API change, specify it, expose completion/errors explicitly, and update all consumers consistently.
- At each ordered `ScreenCopy` operation, preserve the GPU frame state at that point before later operations mutate it. Resolve readback asynchronously and publish results in request order.
- Preserve current capture dimensions, row ordering, green-channel meaning, request limits, overflow reporting, and failure visibility. Do not substitute CPU-reference pixels for a GPU capture.
- Ensure ordinary frame presentation remains GPU-resident and that readback occurs only for actual snapshot/copy requests.
- Add tests for multiple copy operations interleaved with drawing and erase, out-of-order GPU completion, queue saturation, readback failure, cancellation/shutdown, and CPU fallback capture.
- Compare captured bytes/output to the active renderer's displayed frame; do not compare WebGPU captures for pixel identity with CPU captures.

*Exit gate:* every requested capture corresponds to the active backend at the correct operation point and order. CPU fallback captures remain compatible, GPU readbacks are bounded and event-driven, and errors/overflow are visible.

= Grade WG-6 - Browser quality, compatibility, and release evidence

*Goal:* validate the integrated feature in real browsers and record exactly where WebGPU is available and what quality improvement it provides.

*Work:*

- Run the native Rust test/build/lint gates and the `wasm32-unknown-unknown` build with the locked dependency graph and documented feature selection.
- Exercise the WG-0 corpus in the supported real-browser matrix. Record browser/OS/GPU adapter, selected renderer, fallback conditions, and test evidence.
- Verify vector edge quality and stable line appearance at representative canvas sizes and display granularities against the approved WG-0 criterion. Record accepted GPU/CPU pixel differences.
- Exercise start/pause/reset, speed and granularity changes, erase, readiness/prompt behavior, screen copies, resize, WebGPU device loss, fallback, and browser lifecycle while vectors are being produced.
- Measure frame responsiveness, memory use, GPU upload/readback volume, and capture latency. Confirm full-frame readback is not occurring on every animation frame.
- Update the WASM documentation, renderer audit, and handoff with actual feature selection, browser support limits, known differences, and remaining risks.

*Exit gate:* all earlier grades pass; the browser matrix demonstrates WebGPU preference and working CPU fallback; the agreed quality criterion passes; emulator features and capture ordering regressions pass; and performance, limitations, and platform-specific differences are recorded. Do not claim universal WebGPU support beyond tested browsers/devices.

= Cross-grade invariants and validation

- `Machine` remains independent of WebGPU, browser APIs, framebuffer formats, and renderer lifecycle.
- Display commands are consumed in machine order. `BeginErase`, `CompleteErase`, and `ScreenCopy` are not merged, reordered, or inferred from a frame diff.
- The CPU reference remains available throughout development and in the browser fallback. It remains a behavioral and diagnostic reference, not a required pixel oracle for the GPU.
- Readiness and other emulation signals derive from shared display semantics, not anti-aliasing thresholds or sampled GPU pixels.
- A GPU failure is surfaced. Fallback is permitted by the selected product behavior, but it must be observable and must not silently report WebGPU as active.
- Preserve the current supported emulator features and operation contract. Keep newly discovered, unrelated emulator defects out of this renderer track.
- Test at the smallest relevant selector per increment, then run package-level regressions at grade exits. Compile this document with `typst compile`; keep generated PDFs out of the repository unless requested.

Suggested implementation-validation commands from the repository root (confirm package features and selectors against the manifests when WG-1 selects the dependency):

```sh
cargo fmt --all -- --check
cargo test --locked -p tek4051-emulator
cargo test --locked -p tek4051-emulator --no-default-features --lib
cargo clippy --locked -p tek4051-emulator \
  --all-targets --all-features -- -D warnings
cargo check --locked -p tek4051-emulator \
  --target wasm32-unknown-unknown --no-default-features --features wasm
```

These are planned checks, not claims that this document-writing task ran Rust validation. Add GPU-specific native/WASM feature selectors only after the chosen dependency and feature matrix are established.

= Requirements and grade mapping

#table(
  columns: (0.8fr, 2.4fr, 2.8fr),
  stroke: 0.5pt,
  [*ID*], [*Required outcome*], [*Grades / evidence*],
  [WG-REQ-01], [WebGPU visibly improves vector quality under an agreed corpus and criterion; pixels may differ from CPU.], [WG-0 freezes corpus/criterion; WG-2 and WG-6 provide native/browser quality evidence.],
  [WG-REQ-02], [All currently implemented emulator/display behavior and operation ordering remain intact.], [WG-0 contract; WG-2/WG-3 operation and regression tests; WG-6 integrated browser tests.],
  [WG-REQ-03], [CPU reference remains available and serves as fallback.], [WG-1 feature/lifecycle design; WG-3 native selection; WG-4 browser fallback; WG-5 CPU capture tests.],
  [WG-REQ-04], [WASM prefers WebGPU when it can initialize and reports CPU fallback otherwise.], [WG-4 capability, initialization, device-loss, and selected-backend tests; WG-6 browser matrix.],
  [WG-REQ-05], [Snapshots and screen-copy requests match the active renderer at the correct ordered operation point.], [WG-1 capture design; WG-3 session integration; WG-5 ordering/readback tests; WG-6 browser capture checks.],
  [WG-REQ-06], [Rendering remains above `Machine`; readiness is independent of renderer pixels.], [WG-0 event inventory; WG-1 contract; WG-2/WG-3 ownership and readiness tests; WG-6 dependency review.],
  [WG-REQ-07], [Implementation proceeds in the Rust emulator before WASM integration.], [WG-1 through WG-3 native exit gates precede WG-4; WG-4 is blocked until WG-3 passes.],
)

== Status

The WebGPU vector renderer has since been integrated above `Machine` in the Rust emulation runtime and is used by the WASM browser shell when its WebGPU surface initializes. The shell reports the active renderer and retains an explicit CPU reference fallback. This implementation does not by itself complete the plan's quality or capture gates: WG-0's representative corpus and visual review remain open; browser screen-copy readback is not supported synchronously and can trigger a reported CPU fallback; WG-5 active-renderer capture and WG-6 browser quality/compatibility acceptance remain open.

On 19 November, the release WASM build with thin LTO was loaded in the
integrated browser, selected WebGPU, and detected the BASIC prompt at x20.
This confirms the runtime selection path but does not close the visual corpus,
active-renderer screen-copy, device-loss, or cross-browser gates. The separate
#link("./wasm-performance-optimization-report.typ")[WASM performance optimization report]
records the performance results and distinguishes CPU-fallback benchmarks
from the live WebGPU check.

The follow-on browser-shell optimization scope is recorded in
#link("./graded-wasm-webgpu-optimization-plan.typ")[Graded WASM WebGPU Rendering Improvement Plan].
That plan permits browser/test/documentation changes only and explicitly excludes Rust emulator source and manifest changes.
