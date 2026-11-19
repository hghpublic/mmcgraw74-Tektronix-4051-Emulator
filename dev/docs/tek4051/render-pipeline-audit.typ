#set par(justify: true)
#set page(
  margin: 40mm,
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
    let page-number = counter(page).display("1")
    if counter(page).get().first() == 1 {
      align(center)[#page-number]
    } else {
      align(right)[#page-number]
    }
  },
)
#set text(font: "Arial", size: 10pt)
#set heading(numbering: "1.1")

#align(center)[
  #text(size: 17pt, weight: "bold")[Tektronix 4051 Emulator]
  #linebreak()
  #text(size: 21pt, weight: "bold")[Render Pipeline Boundary Audit]
]

#align(center)[
  Review type: API and architecture audit \
  Scope: Rust machine, emulation, native egui, and WASM display paths \
  Target: Higher-quality browser WASM vector rendering with WebGPU \
  Status: WASM WebGPU integration and optional Extended-WebGPU vector zoom implemented; visual corpus and active-renderer capture acceptance remain open \
  Original audit: 17 November 2026; status update: 22 November 2026
]

#outline(title: [Contents], depth: 2)

= Executive conclusion

The implementation has moved rasterization above `Machine`: the machine
provides platform-neutral display operations, while the emulation layer owns
the CPU reference renderer and the integrated WebGPU renderer. The browser
selects WebGPU when initialization succeeds and retains the CPU reference as
an explicit fallback. The goal remains higher vector quality without changing
implemented display behavior.

The emulation-to-frontend output should remain renderer-neutral. The native adapter converts the CPU reference frame to egui data; the WASM WebGPU renderer should consume display operations rather than merely upload the existing CPU bitmap. This is consistent with AR-3's application presentation adapter for frontend-specific surface integration; it does not place the emulated renderer below `Machine`.

The CPU reference renderer preserves the existing pixels and rendering behavior. The WebGPU renderer has a different goal: improve vector-display visual quality, so pixel-level differences from the CPU reference are intentional. Compatibility means preserving implemented emulation features and operation semantics—not pixel-perfect output. The browser should prefer WebGPU when supported, fall back to the CPU renderer otherwise, and make screen copies match the renderer currently displayed.

A host-created `wgpu::Device` and `wgpu::Queue` are supplied to the
emulation-owned renderer; instance, adapter, and device creation stay outside
emulation. The integrated path consumes ordered display operations, retains
transient/stored geometry across redraws, and presents through the browser
WebGPU surface. The browser still uses CPU-reference pixels for screen-copy
requests that require synchronous capture. The user approved qualitative
CPU/GPU comparison at matching output sizes without a numeric threshold; the
WG-0 corpus, active-renderer capture, and final review evidence remain open.

The optional Extended-WebGPU vector camera re-rasterizes the retained scene;
its stroke width is capped at the 1× value while zooming in and continues to
thin when zooming out. Focused renderer tests cover both directions, and a
release-browser probe confirmed that Vector mode initializes without the
bitmap canvas transform. This does not complete the visual corpus,
active-renderer ScreenCopy capture, or browser/device acceptance.

= Scope and evidence

This audit follows the current data path from screen commands through the machine display model, the emulation snapshot, and the native and browser presentation adapters. It also checks the screen-copy path because exported captures must remain correct if the on-screen renderer changes.

Evidence comes from the Rust package under `dev/src/4051-Emulator/rs/4051-Emulator-rs/`, especially `machine/display.rs`, `machine/mod.rs`, `emulation/mod.rs`, `emulation/session.rs`, `native/backend.rs`, `gui/app.rs`, `platforms/wasm/mod.rs`, `platforms/wasm/browser/app.js`, and `host/io.rs`. The existing graded architectural refactoring guide's AR-3 guidance is also relevant: it keeps egui out of emulation and places display conversion in an application presentation adapter (`dev/docs/tek4051/web/design/graded-architectural-refactoring-guide.typ`, lines 580–600).

This remains a source-level API audit and does not itself establish WebGPU
performance, browser compatibility, or GPU device-loss behavior. Runtime
performance measurements are now recorded separately in the
#link("./web/design/wasm-performance-optimization-report.typ")[WASM performance optimization report];
the tested browser/device matrix and GPU device-loss behavior remain
incomplete.

= Current pipeline

== Machine display operations

`Machine` emits an ordered stream of platform-neutral `DisplayOperation`s. Each screen command carries the sampled display-control values; erase begin/completion and screen-copy requests are distinct operations. `Machine::take_display_operations()` drains the queued stream. The machine contains no pixel buffer, granularity setting, rasterizer, or presentation resource (`machine/display.rs`, `machine/bus.rs`, and `machine/mod.rs`). Callers that own an emulation renderer must drain this queue regularly.

The emulation layer's `CpuReferenceRenderer` consumes those operations and owns the fixed 1046 × 830 row-major pixel buffer, green-channel intensities, granularity, dot/vector drawing, phosphor refresh, erase behavior, revisions, semantic readiness events, and event-time screen-copy capture (`emulation/display.rs`). Its wrapping `u64` revision identifies framebuffer changes, so consumers should use it as a change token rather than a never-wrapping sequence number. `HostRuntime` synchronizes operations around CPU execution, refresh callbacks, and erase completion (`emulation/runtime.rs`).

The renderer's readiness `DisplayEvent`s are separate from the machine-operation stream. They remain crate-private, use a bounded queue, and feed the emulation session's cursor-readiness logic; they are not a substitute for rendering operations.

== Emulation snapshot

The public `Snapshot` contains `Arc<[u8]>` pixels, a revision, and display granularity, alongside other session/UI state (`emulation/mod.rs`). `EmulationState::snapshot()` reuses the shared pixel allocation while the revision is unchanged and copies from the emulation-owned CPU reference renderer when it changes (`emulation/session.rs`).

The frame description is implicit, though: `Snapshot` does not carry width, height, row stride, or a pixel-format tag. Consumers must know that the dimensions are the `CpuReferenceRenderer` constants and that each byte is an intensity. The snapshot also bundles session metadata unrelated to rendering. Those choices work for current in-repository clients but make a renderer-facing contract less self-describing.

== Native presentation

The native-only `DisplayPresentation` converts intensity bytes to egui green `Color32` values and caches an `egui::ColorImage` by revision. `PresentedSnapshot` contains that egui image (`native/backend.rs`, lines 39–65). The GUI creates and updates an egui texture using linear filtering and draws it scaled to the display area (`gui/app.rs`, lines 82–85, 132–136, 360).

This keeps egui out of `Machine` and `EmulationState`, as the architecture guidance intends. But the native backend's prepared-output type is egui-specific, so another presentation implementation cannot simply substitute for `DisplayPresentation` while conforming to the same output contract.

== WASM and capture paths

The WASM-facing `BrowserSnapshot` exposes width, height, revision, CPU
reference `pixels()`, and `webgpu_active`. When WebGPU is active, the browser
shows the WebGPU canvas and does not request CPU pixels for ordinary
presentation. In CPU fallback, a changed revision obtains the pixel vector,
converts intensity into green-channel RGBA `ImageData`, and calls
`putImageData`. The CPU pixel accessor still copies into JavaScript-owned
memory; do not assume a zero-copy path for CPU fallback or screen captures.

Screen saving has its own intensity-to-RGBA conversion before PNG encoding (`host/io.rs`). Screen copies must match the selected display renderer and preserve event-time images. The CPU reference can capture directly; WebGPU needs readback only when a capture is requested.

= Boundary assessment

== What is already well separated

- The machine emits ordered display operations without owning rasterized display state.
- The emulation snapshot is immutable to consumers and shares pixel storage until the framebuffer changes; it publishes the CPU reference renderer's output.
- Native and browser presentation conversions live outside the machine display model.
- The screen-copy path is independent of the on-screen texture/canvas.

Together, these seams provide a stable CPU-rendered frame for the existing frontends. The native GUI is not a public renderer extension point, and the browser currently has its own adapter; a shared renderer lifecycle or backend abstraction is not yet present.

== Gaps to address for a reusable renderer boundary

- *Frame metadata is implicit.* Width, height, row layout, and intensity semantics are not expressed together with the bytes in `Snapshot`.
- *Native prepared output is platform-bound.* The current backend output includes `egui::ColorImage`, which prevents the same backend contract from representing a WebGPU texture or another presentation resource.
- *There is no shared renderer lifecycle contract.* The code does not define a renderer-facing interface for accepting a new frame, updating a surface, scaling/filtering, or reporting renderer errors. Each frontend currently owns its own presentation behavior.
- *Display events are not a substitute.* Their visibility, bounded queue, and current readiness-oriented consumer make them unsuitable as an external renderer contract. They should not be promoted as one without specifying ordering, loss, and replay semantics.
- *Cross-platform conversions are separate.* Native egui, browser canvas, and PNG export each map intensity bytes to their destination format. A common frame format could clarify the input semantics, but should not make emulator core depend on a particular GPU or UI format.

= Recommended design

== Preserve the emulation-layer renderer boundary

The renderer that interprets display activity and maintains the emulator's visible display state belongs in the emulation layer, above `Machine`. The machine now provides a platform-neutral ordered display-operation contract and does not own rasterization. The contract preserves sampled coordinates/control values, erase behavior, screen-copy requests, and ordering needed by the CPU reference renderer.

The existing CPU rasterization behavior lives in `CpuReferenceRenderer` and
produces the pixel frame used by CPU snapshots and fallback captures. The
integrated WASM WebGPU renderer consumes ordered display operations and keeps
platform-specific device/surface lifecycle in the browser adapter. Active
renderer screen-copy output remains incomplete: a capture can trigger an
explicit CPU fallback. AR-3's separation of egui-specific conversion from
emulation remains applicable.

== Make the frame contract explicit

Before treating the boundary as a reusable extension point, introduce a small immutable frame value, either as a member of `Snapshot` or as a separate accessor. It should make the currently implicit facts explicit:

- width and height;
- row stride in bytes;
- pixel format and intensity interpretation;
- a frame revision with documented change and overflow semantics;
- immutable, shared pixel storage.

For the current reference-rendered output, a single-channel 8-bit intensity format is sufficient. Preserve full-frame update and revision behavior during the ownership move; do not combine this refactor with dirty-region updates or other feature changes. Consumers can use the revision to avoid redundant uploads; they should not infer that every revision corresponds to a partial update.

== Keep frontends focused on presentation

Native and browser frontends should consume the emulation renderer's output and handle only the host-facing presentation integration. An egui frontend can update its texture; a browser frontend can present through Canvas 2D or a WebGPU surface. If a GPU implementation is selected as the emulation renderer, pass only the minimal platform surface/context through a narrow adapter; the rendering contract and emulated display behavior remain owned above `Machine`.

When adapting the native worker path, preserve its separation between emulation updates and GUI texture mutation. GPU resource creation and upload must follow the owning renderer's context/thread requirements, while the emulation renderer remains independent of egui. Keep only the necessary window/surface constraints in the platform adapter.

For WASM, WebGPU is intended to rasterize the ordered display operations at improved vector quality, not just replace Canvas 2D presentation of the CPU frame. The emulation layer should retain renderer ownership; browser-specific device and surface lifecycle belongs in a narrow adapter. Do not trade a clear ownership contract for an assumed zero-copy optimization.

== Preserve capture and output behavior

Generate screen captures from the selected renderer so they match the displayed output; for WebGPU, perform readback only when a capture is requested. Validate dimensions, vector origins, operation ordering, intensity/erase semantics, and the intended quality improvement. Keep native linear filtering intentional and select and test WebGPU sampling explicitly.

= Implementation and validation sequence

1. *Display-operation contract:* implemented as an ordered machine-to-emulation operation stream with sampled controls, erase transitions, and screen-copy requests.
2. *CPU reference renderer:* moved above `Machine`; focused regressions cover vectors, granularity, refresh/erase, revisions, event-time screen copies, and overflow.
3. *Frontend integration:* native and browser presentation continue to consume emulation snapshots; egui and browser conversion remain outside the machine and renderer.
4. *WebGPU implementation:* implemented in the emulation layer with browser device/surface integration, explicit renderer status, and CPU fallback.
5. *Behavior checks:* CPU-reference, operation-order, and fallback regressions are implemented. The accepted WebGPU quality corpus, active-renderer capture, and full browser/device matrix remain open; performance measurements are summarized in the separate report.

The release WASM build was confirmed running with WebGPU selected and BASIC
prompt readiness at x20 in the integrated browser. A separate report records
CPU-fallback x100 benchmarks and build-profile comparisons. These measurements
do not establish x100 WebGPU performance or close the quality, capture, or
cross-browser acceptance gates.

= Final assessment

*Renderer integration readiness: partial.* `Machine` emits ordered display
operations; the emulation layer owns the CPU reference and WebGPU renderers;
and the browser selects WebGPU when available with visible CPU fallback.
Release-profile WebGPU and Vector mode selection are confirmed in the
integrated browser, and performance evidence is recorded separately. No
matched runtime measurement of vector-zoom rerasterization has been collected.
WG-0's approved visual corpus,
active-renderer screen-copy readback, device-loss recovery, and supported
browser/device acceptance remain open. The browser CPU pixel API remains a
fallback/capture path rather than a GPU readback contract.
