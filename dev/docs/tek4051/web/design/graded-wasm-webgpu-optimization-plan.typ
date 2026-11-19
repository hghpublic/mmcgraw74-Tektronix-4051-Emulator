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
  #text(size: 15pt)[Graded WASM WebGPU Rendering Improvement Plan]
]

#align(center)[
  Scope: browser-shell presentation and browser tests/documentation only
  \
  Excluded: Rust emulator source, Cargo manifests, and Rust/WASM API changes
  \
  Status: WGP-0 current-state characterization complete; WGP-1 implemented; WGP-2 lifecycle coverage in progress
  \
  Updated: 19 November 2026
]

#outline(title: [Contents], depth: 2)

= Purpose

This follow-on plan improves the browser-side efficiency and responsiveness of
the existing WASM WebGPU presentation path. Every implementation change in
these grades must remain outside the Rust emulator source and Cargo manifests.
The WebGPU vector renderer and `BrowserEmulator` API are treated as fixed
interfaces for this track.

The current browser loop calls `emulator.advance()` synchronously and then
calls `present_webgpu()` on every animation frame. A `BrowserSnapshot` exposes
a display `revision`; the CPU canvas already skips unchanged revisions, while
the WebGPU presenter does not. The first implementation opportunity is
therefore to avoid redundant browser surface presentations without changing
the emulated frame or reading it back to JavaScript.

This is a constrained optimization track, not a promise that JavaScript can
remove costs inside a long synchronous Rust/WASM advance call. If measurements
show that the remaining bottleneck requires Rust renderer changes, this plan
stops at that boundary and records the work for separate authorization.

= Scope and invariants

== Allowed changes

- Browser-shell files under
  `dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser/`.
- Browser tests and the opt-in performance benchmark under
  `dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser-tests/`.
- Directly related Typst documentation and this plan.
- Browser-side instrumentation that uses existing snapshot and presentation
  APIs without changing their signatures or semantics.

== Excluded changes

- Any Rust source in the emulator crate, including `emulation/`, `machine/`,
  and `platforms/wasm/*.rs`.
- `Cargo.toml`, `Cargo.lock`, new Rust/WASM exports, or changes to
  `BrowserSnapshot` and `BrowserEmulator`.
- Changes to CPU execution, display-operation generation, cursor semantics,
  readiness, input sequencing, ROMs, or supplied programs.
- Full-frame GPU readback or CPU-pixel upload as a WebGPU presentation
  workaround.
- A worker-based emulator or renderer migration that requires changing the
  Rust/WASM boundary.

== Cross-grade invariants

- WebGPU remains selected only while usable; CPU fallback and its visible
  status/error remain functional.
- The first WebGPU frame, each changed display revision, and each required
  redraw after visibility or surface changes must be presented.
- A skipped presentation may never leave a stale or blank display.
- The ordinary WebGPU presentation path performs no full-frame readback.
- Browser input remains routed through the existing keyboard adapter.
- Do not infer machine or BASIC acceptance from browser event-dispatch timing.
- Do not claim a performance improvement unless paired measurements show it
  beyond ordinary run-to-run variation.

= Evidence and current diagnosis

Three paired headed Chrome 158 runs at 1280x720 used matching 2.9-second
windows and speed order. BASIC prompt/cursor budget use at x20 had medians and
ranges of 229.22% [228.22%, 234.18%] with WebGPU and 99.07% [96.36%, 100.32%]
with CPU fallback. At x50 it was 574.02% [561.42%, 578.46%] versus 244.85%
[241.17%, 247.25%]; at x100 it was 1119.15% [1114.30%, 1145.48%] versus
487.08% [482.55%, 487.20%]. At x100, average frame intervals were 109.55 ms
[104.76, 111.31] versus 48.44 ms [48.42, 48.97]. The vector loop at x100 was
nearly equal: 550.78% [550.28%, 551.39%] versus 544.73% [541.47%, 546.40%].

At x100 the WebGPU presenter averaged 0.062 ms [0.056, 0.063] per call, with
8-9 actual presentations and 34-35 unchanged-frame skips per run. Thus the
revision gate avoids most redundant presentation attempts, but measured BASIC
advancement and key-dispatch latency did not improve comparably. Single-key
page-to-handler delays were 325.6 ms [228.3, 398.8] with WebGPU and 149.8 ms
[147.6, 151.2] with CPU fallback; these noisy host-side samples do not measure
PIA delivery or BASIC acceptance. Presenter instrumentation was introduced
with revision-aware presentation, so no equivalently instrumented pre-change
call-count/timing series exists; skipped counts characterize current avoided
attempts, not a direct before/after presenter timing comparison.

The measurements localize the additional cost to the synchronous
`emulator.advance()` path rather than JavaScript `present_webgpu()` time. Two
opt-in, five-second Chrome CPU profiles were also captured at the x100 BASIC
prompt/cursor workload, once with WebGPU and once with forced CPU fallback.
The WebGPU profile sampled `HostRuntime::sync_display` 2,357 times,
`WebGpuVectorRenderer::record_segments` 2,083 times, and
`segment_instance_bytes` 1,293 times. The CPU-fallback profile sampled
`HostRuntime::sync_display` 379 times, `HostRuntime::apply_display_operation`
374 times, and `CpuReferenceRenderer::apply_screen_command` 148 times.

These are inclusive profile samples: parent and child frames overlap, so their
counts or sampled milliseconds must not be added. Profiler overhead also
makes the displayed sampled milliseconds unsuitable as elapsed-time
comparisons. The paired profiles directly identify WebGPU segment preparation
and serialization as substantial WASM-side work in this workload, consistent
with the larger WebGPU cost in the BASIC prompt/cursor benchmark. The profiles
do not identify the origin of each segment or isolate each function's cost;
they therefore do not prove that repeated transient cursor geometry alone
causes the slowdown. Earlier native profiling implicated that geometry, but
the exact WASM contribution remains unresolved.

After this browser-only diagnosis, a separate Rust-side profiling/optimization
pass was explicitly authorized. It is not part of the grades or an expansion
of this plan's allowed change boundary. That follow-up caches packed transient
segment instance bytes as new geometry is appended and reuses them on redraw;
refresh and complete erase clear the cache. Display-operation segments and
flags are committed only after transient packing succeeds. Three paired headed Chrome 158
benchmark runs at 1280x720 measured BASIC prompt/cursor medians of 138.84% /
102.17% of the 10 ms budget (WebGPU / CPU fallback) at x20, 340.43% / 250.62%
at x50, and 677.23% / 495.77% at x100. Compared with prior WebGPU medians
(229.22%, 574.02%, and 1119.15%), the measured WebGPU cost fell about 39-41%;
the vector-loop results remained near their earlier levels. These results are
host-specific, not thresholds.

The final paired five-second x100 profiles sampled WebGPU
`HostRuntime::sync_display` 1,290 times, `PendingDisplayGeometry::commit` 810
times, `TransientGeometry::append` 742 times, and
`append_transient_segment_bytes` 708 times. CPU fallback sampled
`HostRuntime::sync_display` 380 times, `HostRuntime::apply_display_operation`
363 times, and `CpuReferenceRenderer::apply_screen_command` 164 times. The previous full-list
serialization and segment-recording frames no longer appeared among the
leading WebGPU frames. Counts are inclusive and descriptive, not additive
elapsed-time measures. The renderer still creates and uploads a GPU vertex
buffer containing the accumulated packed bytes on redraw. A separate
profiler-free Playwright diagnostic measured segment-buffer creation,
mapped initialization through unmap, and queue submission over three headed
Chrome 158 runs at 1280x720. Median synchronous host-side costs were 2.2 ms
[1.1, 2.5] for `createBuffer`, 88.5 ms [88.1, 88.9] for mapped initialization,
and 1.2 ms [0.8, 1.8] for `queue.submit` over 5.62-5.67 second windows
(approximately 1.6% combined). The post-pause queue-completion wait median was
13.0 ms [9.0, 14.4]; it is one tail wait per run, not a per-frame GPU timing.
This makes synchronous buffer setup unlikely to explain the remaining x100
WebGPU cost by itself, but asynchronous GPU completion remains unresolved.
The next profile target is append-time geometry preparation/validation, not
buffer reuse without further evidence. Subsequent cursor-deduplication and
shared-CPU results are recorded below and in the separate
#link("./wasm-performance-optimization-report.typ")[WASM performance optimization report].

== Separate authorized follow-up outside this plan

The browser-only grades and exclusions above remain unchanged. A later
separately authorized Rust pass added per-refresh deduplication of identical
WebGPU cursor marks, then profiled and optimized shared CPU execution. The
cursor experiment measured a single-run x100 BASIC decrease from 690.33% to
535.71% of the 10 ms budget; suppressing all remaining cursor drawing reduced
that only to 528.26%, so broader suppression was rejected. After the shared
CPU changes, three-run CPU-fallback x100 medians reached 240.39% BASIC and
296.06% vector loop in the development build. Optimized release medians were
40.88% and 44.60%; optional thin LTO measured 38.47% and 42.40%. These are
host-specific `emulator.advance()` values, not total CPU utilization or
WebGPU x100 results. The detailed complexity and rejected-change record is in
the linked report.

= Grade overview

#table(
  columns: (0.7fr, 1.8fr, 2.8fr, 1.0fr),
  stroke: 0.5pt,
  [*Grade*], [*Boundary gained*], [*Primary exit evidence*], [*Status*],
  [WGP-0], [Repeatable baseline and timing split], [Three paired current-state runs report advancement, presenter, frame, and input-dispatch medians and ranges.], [Complete (current state)],
  [WGP-1], [Revision-aware surface presentation], [Unchanged revisions skip; startup, changed revisions, and lifecycle redraws are tested.], [Complete],
  [WGP-2], [Browser lifecycle and fallback coverage], [Resize, visibility, pageshow, adapter failure, and CPU fallback are covered; remaining lifecycle matrix is open.], [In progress],
  [WGP-3], [Measured browser-only acceptance], [Revision-aware presentation skips unchanged frames; complete paired WebGPU acceptance remains open. Later authorized runtime evidence is summarized separately.], [In progress],
  [WGP-4], [Rust boundary stop/defer decision], [The browser-only boundary is preserved; a separate authorized Rust follow-up is documented outside these grades.], [Boundary recorded],
)

Each grade is gated by the previous grade. A grade may conclude that no
browser-only optimization is material; that is a valid result if the evidence
is complete and the scope boundary is preserved.

= Grade WGP-0 - Establish repeatable browser baselines

*Goal:* separate browser surface-presentation cost from synchronous emulator
advancement and measure the browser's key-dispatch delay under comparable
conditions.

*Work:*

- Preserve the existing BASIC prompt/cursor and vector-loop workloads, speed
  settings, WebGPU/forced-CPU modes, frame intervals, and key-dispatch probe.
- Instrument the existing `present_webgpu()` call from JavaScript with
  duration and call-count measurements. Record display revisions, revisions
  presented, and frames skipped, without adding Rust APIs.
- Run at least three paired WebGPU and forced-CPU samples in the same headed
  browser, viewport, workload, and speed order. Record browser/OS, selected
  renderer, sample coverage, and any outlier interruptions.
- Report medians and ranges for `emulator.advance()`, presenter calls, whole
  frames, and page-to-key-handler delay. Keep Event Timing's 16 ms reporting
  cutoff explicit.

*Exit gate:* three paired, environment-qualified samples distinguish
advancement from presentation cost and report observed ranges. They establish
host-specific variation, not portable release thresholds.

= Grade WGP-1 - Skip redundant WebGPU surface presentations

*Goal:* avoid presenting the same display revision repeatedly using only the
existing browser snapshot contract.

*Work:*

- Track the last successfully presented `snapshot.revision` in the browser
  shell. Present the initial frame and a frame whose revision has changed;
  skip repeated calls for the same revision.
- Reset the browser-side presentation cache when WebGPU becomes inactive,
  the page becomes visible after being hidden, or a surface/canvas lifecycle
  event requires a redraw. Do not mark a revision presented if the call
  failed.
- Keep fallback rendering independent and usable. Do not access
  `snapshot.pixels()` while WebGPU is active.
- Add browser tests for initial presentation, unchanged revisions, changed
  revisions, fallback transitions, and forced redraw after visibility or
  surface changes. Instrument surface acquisition or the browser call path
  without adding a Rust test hook.

*Exit gate:* unchanged revisions do not cause repeated surface presentation;
the first frame and each required changed/recovery frame are visible; failures
remain explicit; CPU fallback and input behavior are unchanged.

= Grade WGP-2 - Validate browser lifecycle and fallback

*Goal:* ensure revision-based presentation remains correct across browser
events and WebGPU availability changes.

*Work:*

- Exercise WebGPU available, API unavailable, and initialization failure
  paths. Confirm that CPU rendering remains usable and the selected-renderer
  status is accurate.
- Exercise viewport resize, tab/page visibility changes, start/pause/reset,
  display updates while the Tools panel is open, and repeated fallback.
- Confirm no per-frame GPU readback or CPU bitmap upload is introduced.
- Treat device-loss recovery as supported only if the existing browser API
  reports a recoverable state. Otherwise preserve the visible CPU fallback
  and record recovery as outside the current API boundary.

*Exit gate:* the visible frame is never stale or blank through supported
lifecycle transitions, fallback errors are visible, and all existing focused
layout, renderer, and input browser tests pass.

= Grade WGP-3 - Measure browser-only performance and quality

*Goal:* determine whether the browser-shell change materially reduces
redundant presentation work and improves observable responsiveness.

*Work:*

- Repeat the WGP-0 paired benchmark after WGP-1/WGP-2 using the same workload
  and browser conditions.
- Compare presenter call count/time, changed-revision coverage, whole-frame
  intervals, and key-handler delays. Compare distributions rather than a
  single result.
- Verify that the displayed vector frame is unchanged for a given renderer
  revision; CPU and WebGPU pixel identity is not a requirement.
- Record any gains separately for idle/unchanged frames and active cursor or
  vector workloads. Do not attribute a lower presenter cost to faster
  `emulator.advance()` work.

*Exit gate:* either a repeatable browser-side improvement is demonstrated with
no stale-frame, fallback, input-routing, or quality regression, or the
measurements show that presentation savings are immaterial. State which
outcome occurred; do not generalize beyond tested browsers and devices.

= Grade WGP-4 - Stop at the Rust boundary

*Goal:* prevent browser-only workarounds from silently expanding into Rust
emulator changes.

*Work:*

- If `emulator.advance()` remains the dominant cost, record that browser
  scheduling cannot preempt the synchronous call.
- If cursor-heavy WebGPU work remains materially slower than CPU fallback,
  record the observed Rust-side transient-geometry path and candidate
  follow-up ideas separately from this plan.
- Do not edit Rust source, Cargo manifests, or the Rust/WASM API under this
  grade. Request a separate scope decision before any such implementation.
- A separate authorization was later granted for Rust-side transient-geometry
  and CPU execution work. That follow-up does not alter this plan's boundary
  or grade status; see the linked optimization report.

*Exit gate:* this track ends with the browser-only results and a clear,
evidence-backed boundary statement. Any Rust renderer optimization requires a
newly authorized plan or scope.

= Validation commands

Run browser tests from
`dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser-tests/`.
These commands use the existing generated WASM package; building Rust/WASM is
not part of this plan unless generated bindings are missing, and then it is
validation only.

```sh
RUN_WASM_PERF_BENCHMARKS=1 npx playwright test \
  -c playwright.config.mjs performance.spec.mjs --headed

WASM_FORCE_CPU_RENDERER=1 RUN_WASM_PERF_BENCHMARKS=1 \
  npx playwright test -c playwright.config.mjs performance.spec.mjs --headed

npx playwright test -c playwright.config.mjs \
  layout.spec.mjs renderer.spec.mjs input.spec.mjs

RUN_WASM_CPU_PROFILE=1 npx playwright test -c playwright.config.mjs \
  performance.spec.mjs --grep "profile of the x100 BASIC prompt" --headed
```

Compile the plan from the repository root with
`typst compile dev/docs/tek4051/web/design/graded-wasm-webgpu-optimization-plan.typ /tmp/graded-wasm-webgpu-optimization-plan.pdf`.
Keep generated PDFs out of the repository unless requested.

= Requirements and grade mapping

#table(
  columns: (0.9fr, 2.5fr, 2.7fr),
  stroke: 0.5pt,
  [*ID*], [*Required outcome*], [*Grades / evidence*],
  [WGP-REQ-01], [All planned implementation changes remain outside Rust emulator source, Cargo manifests, and Rust/WASM API definitions.], [Scope limits and WGP-0 through WGP-4 boundary checks.],
  [WGP-REQ-02], [Avoid redundant WebGPU surface presentation when the displayed revision is unchanged.], [WGP-0 timing/call baseline; WGP-1 revision gate and browser tests.],
  [WGP-REQ-03], [Present the first frame and every changed or required recovery frame without stale output.], [WGP-1 initial/change tests; WGP-2 lifecycle tests.],
  [WGP-REQ-04], [Preserve CPU fallback, visible errors, and browser keyboard routing.], [Cross-grade invariants; WGP-1 and WGP-2 regressions.],
  [WGP-REQ-05], [Do not add full-frame readback to ordinary WebGPU presentation.], [Cross-grade invariants; WGP-2 inspection and tests.],
  [WGP-REQ-06], [Measure advancement, presentation, frame delay, and browser key dispatch separately.], [WGP-0 instrumentation and WGP-3 paired results.],
  [WGP-REQ-07], [Defer any optimization requiring Rust changes until separately authorized.], [WGP-4 stop/defer decision.],
)
