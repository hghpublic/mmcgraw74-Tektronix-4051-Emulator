#set par(justify: true)
#set page(
  margin: 40mm,
  header-ascent: 18pt,
  footer-descent: 18pt,
  header: context {
    if counter(page).get().first() > 1 {
      let prior = query(heading.where(level: 1, outlined: true))
        .filter(heading => heading.location().page() <= here().page())
      let label = if prior.len() > 0 { prior.last().body } else { [] }
      grid(
        columns: (1fr, auto, 1fr),
        gutter: 0pt,
        align: (left, center, right),
        [],
        label,
        counter(page).display("1"),
      )
    }
  },
  footer: context {
    if counter(page).get().first() == 1 {
      align(center)[#counter(page).display("1")]
    }
  },
)
#set text(font: "Arial", size: 10pt)
#set heading(numbering: "1.1")

#align(center)[
  #text(size: 20pt, weight: "bold")[Tektronix 4051 Emulator]
  \
  #text(size: 15pt, weight: "bold")[WASM Performance Optimization Report]
]

#align(center)[
  Scope: browser WASM WebGPU rendering and shared 6800 CPU execution
  \
  Evidence window: 18-19 November 2026
  \
  Status: measured improvements retained; unproven complexity rejected or deferred
  \
  Updated: 19 November 2026
]

#outline(title: [Contents], depth: 2)

= Executive summary

The largest confirmed x100 improvement came from using an optimized WASM
release build rather than Cargo's default development profile. With the same
emulator source and CPU-fallback workloads, release medians fell from 240.39%
to 40.88% of the 10 ms advance budget for the BASIC prompt/cursor, and from
296.06% to 44.60% for the vector loop. An optional thin-LTO build reduced
those medians further to 38.47% and 42.40%, while producing a 4.6% smaller
generated WASM binding.

Several focused source changes also produced repeatable CPU-fallback gains:
direct PIA interrupt checks, high-nibble opcode-family dispatch, and a RAM
read fast path. On the WebGPU path, cached transient-geometry packing and
per-refresh duplicate cursor-mark suppression reduced repeated preparation
and upload work. The accepted changes preserve the CPU reference behavior,
display operation order, readiness events, and explicit CPU fallback.

Not every measured hotspot justified more code. GPU buffer pooling, a broader
10 Hz cursor throttle, suppressing all cursor-position drawing, a more
fine-grained opcode route, and global LTO defaults were rejected or left
optional based on measured value, behavior risk, or unvalidated build tradeoffs.

The current release WASM has been run in the integrated browser with WebGPU
selected and BASIC at the prompt. That runtime check was at x20; the x100
thin-LTO benchmark was forced to CPU fallback because WebGPU was unavailable
in the benchmark browser. These results must not be presented as a paired
x100 WebGPU-versus-CPU comparison.

= Measurement method and limits

The browser benchmark reports synchronous `emulator.advance()` wall time as a
percentage of one 10 ms budget. It is an emulation-call cost, not total browser
CPU utilization, end-to-end frame latency, or a portable release threshold.
Values above 100% mean the measured call exceeded that budget.

The three-run CPU benchmarks used headless Chrome 158 at 1280×720. The
WebGPU renderer profile and cursor-geometry experiments used headed Chrome
158 at the same viewport. WebGPU was unavailable in the headless CPU
benchmark, so the CPU and LTO results explicitly forced the CPU fallback.
The final live release run separately confirmed the WebGPU renderer at x20.

Three-run medians and ranges are used where available. Single-run cursor
experiments are directional only. Chrome profile measurements are sampled
time: inclusive parent and child frames overlap, and self samples are not
elapsed-time comparisons. The five-second release CPU profile also included
about 3.5 seconds of idle samples; its function times identify candidate
hotspots but are not a benchmark.

= Optimization results

== Shared 6800 CPU execution

#table(
  columns: (1.35fr, 1.15fr, 1.15fr, 2.35fr),
  stroke: 0.5pt,
  [*Stage*], [*BASIC prompt/cursor*], [*Vector loop*], [*Change and interpretation*],
  [Initial development build], [485.51%], [542.75%], [Three-run CPU-fallback median of the 10 ms budget.],
  [Direct PIA checks], [274.03%], [340.99%], [Replaced fixed-size iterator scans with explicit short-circuit checks over the same PIAs and order.],
  [High-nibble dispatch], [250.98%], [313.88%], [Avoided probing unrelated opcode-family decoders; about 8.4% and 8.0% below the preceding stage.],
  [RAM read fast path], [240.39%], [296.06%], [Addresses below ROM_BASE return RAM before I/O and ROM-bank checks; about 4.2% and 5.7% below the preceding stage.],
  [Standard release], [40.88%], [44.60%], [Same source, optimized Cargo release profile; about 83% and 85% less call time than development.],
  [Release with thin LTO], [38.47%], [42.40%], [Three-run medians with one codegen unit; about 5.9% and 4.9% below standard release.],
)

The PIA iterator closures were prominent in the initial five-second profile:
575 ms for `irq_pending` and 448 ms for `nmi_pending`, as inclusive samples.
The direct checks preserve the same routed PIAs and order. The high-nibble
dispatch retains the explicit supported/unsupported opcode handling. The RAM
fast path is valid because the I/O range (0x8780–0x87ff) is above ROM_BASE
(0x8000), so addresses below ROM_BASE can return RAM immediately.

The development-to-release comparison was the largest single improvement
and did not require a source change. Thin LTO used
`CARGO_PROFILE_RELEASE_LTO=thin` and
`CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1`. The resulting generated binding WASM
was 1,407,620 bytes, versus 1,474,987 bytes for standard release. These
results are host-specific; no global Cargo profile defaults were changed.

== WebGPU display preparation and presentation

#table(
  columns: (1.45fr, 1.65fr, 3.2fr),
  stroke: 0.5pt,
  [*Change*], [*Complexity added*], [*Measured result and decision*],
  [Revision-aware presentation], [Low: retain the last presented display revision and skip unchanged frames.], [At x100, 8–9 actual presentations and 34–35 skipped attempts per run; each synchronous presenter call averaged 0.062 ms. Kept because it avoids redundant presentation without changing frames. No equivalently instrumented pre-change timing series exists.],
  [Compact segment instances], [Moderate: encode one compact instance and expand its antialiased quad in the vertex shader.], [Reduced geometry data per segment from 192 to 32 bytes (6× smaller). This is a structural upload reduction; do not treat it alone as a measured end-to-end speedup.],
  [Incremental transient-geometry packing], [Moderate: retain packed bytes and a segment count; stage operations until packing succeeds; clear both together on refresh/erase.], [Three paired runs reduced WebGPU BASIC budget medians about 39–41% at x20/x50/x100 versus prior WebGPU baselines. Profile frames for repeated full-list serialization were no longer leading. Kept; operation order and CPU rendering remain unchanged.],
  [Cursor-mark deduplication], [Moderate: keep a per-refresh set keyed by position and intensity; clear it with transient geometry.], [One-run BASIC measurements fell about 22% at x20/x50/x100 (690.33% to 535.71% at x100). The vector loop remained broadly similar. Kept because identical marks are idempotent under max blending; CPU pixels and readiness are untouched.],
)

The packed transient cache reduced profile time in
`TransientGeometry::append` from 921 ms to 65 ms inclusive, and
`append_transient_segment_bytes` from 879 ms to 31 ms, in the cited
before/after profiles. These are nested inclusive samples, not additive
elapsed times. After cursor deduplication, three buffer-metric runs recorded
10 buffers and 25,600 uploaded bytes, versus prior medians of 83 buffers and
133,317,760 bytes. Mapped initialization fell from 88.5 ms to 0.3 ms over
roughly 5.5 seconds. This made additional buffer pooling still less
compelling.

= Complexity and rejected or deferred changes

#table(
  columns: (1.6fr, 1.8fr, 2.9fr),
  stroke: 0.5pt,
  [*Proposal*], [*Complexity or risk*], [*Evidence and decision*],
  [Pool WebGPU segment buffers], [Capacity tracking, in-flight GPU resource lifetime, and queue-order correctness.], [Before deduplication, createBuffer cost was 2.2 ms and mapped initialization 88.5 ms per 5.62–5.67 second window. After deduplication, only 10 buffers / 25,600 bytes and 0.3 ms mapped initialization remained. Pooling was not implemented; its residual target was too small to justify the extra lifecycle state.],
  [Throttle cursor marks to 10 Hz], [Could suppress changed positions or intensities and conflate cursor drawing cadence with the 100 ms phosphor refresh.], [No independent timing evidence showed a broader throttle was needed. Exact duplicate suppression already removes repeated identical marks while preserving changed marks. Rejected as a behavior-affecting workaround.],
  [Ignore all drawing at the cursor position], [Would intentionally remove displayed graphics at that location.], [After deduplication, a one-run x100 BASIC sample improved only from 535.71% to 528.26% (1.4%); vector-loop cost was unchanged. Rejected as too little benefit for visible output loss.],
  [Route groups 8-F by low opcode nibble], [Adds another dispatch rule and requires preserving unsupported-opcode fallthrough.], [Three-run medians were 40.68%/44.33% versus baseline 40.88%/44.60%, with overlapping ranges. The 0.20/0.27 percentage-point differences were not meaningful; the trial was reverted.],
  [Further bus/decoder specialization], [Would add fast-path state or more complex mapping/dispatch logic; unsafe unchecked access would also weaken bounds safety.], [The release profile still sampled CPU stepping and bus reads, but no specific low-risk implementation had demonstrated a repeatable gain. Deferred rather than optimizing a hotspot name alone.],
  [Make thin LTO a global Cargo default], [Could change clean build/link time and affect developers or targets beyond this WASM workload.], [The WASM benchmark improved and the output was smaller, but cross-host, cross-target, and clean-build costs were not established. Kept as an opt-in command; Cargo defaults remain unchanged.],
)

The buffer measurements also showed a 1.2 ms median synchronous
`queue.submit` cost over the initial 5.6-second window and a 13.0 ms
post-pause queue drain. The latter is a single tail wait, not a per-frame GPU
timing. After deduplication, the tail drain was 0.6 ms. These measurements do
not isolate GPU execution time and do not justify redesigning queue
synchronization.

= Current state and next evidence

The optimized release WASM was loaded in the integrated browser on 19 November
2026. The Display status reported WebGPU and BASIC prompt readiness; the latest
snapshot showed the emulator running at x20. This confirms a real WebGPU
runtime path, not x100 WebGPU performance acceptance.

The remaining CPU-fallback release profile identified `Cpu6800::step` and
`MachineBus::read_byte` as leading emulator self-sample frames (443.6 ms and
261.1 ms in a five-second capture). A low-nibble dispatch trial did not
improve benchmark medians, so the retained implementation stays unchanged.
Further source optimization should require a concrete mechanism and a
repeatable release-profile gain.

Remaining limits include the incomplete real-browser/device matrix, open
visual-quality and active-renderer screen-copy acceptance, and the lack of a
matched x100 WebGPU benchmark in a browser where WebGPU is available. The
historical WebGPU and CPU-fallback results must not be added together or
compared across unmatched build profiles.

= Reproduction

Build the standard optimized WASM package and generate its browser bindings:

```sh
cargo build --locked --release --target wasm32-unknown-unknown \
  -p tek4051-emulator --no-default-features --features wasm --lib
wasm-bindgen --target web \
  --out-dir target/wasm/pkg \
  target/wasm32-unknown-unknown/release/tek4051_emulator.wasm
```

For the measured thin-LTO variant, prefix the build with
`CARGO_PROFILE_RELEASE_LTO=thin CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1`.
Run the opt-in Playwright benchmark from
`dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser-tests/`:

```sh
WASM_FORCE_CPU_RENDERER=1 RUN_WASM_PERF_BENCHMARKS=1 \
  npx playwright test -c playwright.config.mjs performance.spec.mjs \
  --grep "reports WASM BASIC and vector-drawing workload by CPU speed" \
  --repeat-each=3 --workers=1
```

= Related documentation and evidence

- #link("../../../../src/4051-Emulator/rs/4051-Emulator-rs/docs/wasm.typ")[WASM build and runtime guide]
- #link("./graded-wasm-webgpu-optimization-plan.typ")[Browser-only WebGPU optimization plan]
- #link("./graded-wasm-webgpu-renderer-plan.typ")[WebGPU renderer implementation plan]
- #link("./graded-wasm-browser-parity-plan.typ")[WASM browser parity plan]
- #link("../../render-pipeline-audit.typ")[Render pipeline boundary audit]
- Source evidence: `machine/bus.rs`, `machine/cpu.rs`, `emulation/webgpu.rs`, `platforms/wasm/browser/app.js`, and `platforms/wasm/browser-tests/performance.spec.mjs`.
