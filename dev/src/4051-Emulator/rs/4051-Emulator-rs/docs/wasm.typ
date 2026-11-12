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
        [],
      )
    }
  },
  footer: context {
    let page_number = counter(page).display("1")
    if counter(page).get().first() == 1 {
      align(center)[#page_number]
    } else {
      align(right)[#page_number]
    }
  },
)
#set text(font: "Arial", size: 10pt)
#set heading(numbering: "1.1")

#align(center)[
  #text(size: 20pt, weight: "bold")[Tektronix 4051 Emulator]
  \
  #text(size: 15pt)[Browser WebAssembly API]
]

#align(center)[
  Rust package: `dev/src/4051-Emulator/rs/4051-Emulator-rs`
  \
  Scope: WASM bindings, browser shell, and browser validation
  \
  Updated: 21 November 2026
]

#outline(title: [Contents], depth: 2)

= Browser WASM API

The `wasm` feature exposes a `wasm-bindgen` session API and the optional shared
Rust WebGPU vector renderer without the native window or dialogs. The browser
shell owns DOM event lifecycle and canvas presentation, prefers WebGPU when its
adapter and device initialize, and retains the CPU reference renderer as an
observable fallback. `map_browser_key` applies the shared Rust mapping rules to
a browser logical key and optional paired text. The shell uses
`map_browser_key_with_shift` to also account for the host layout's event-time
Shift modifier.

The graded plan for adding browser keyboard and Flash Drive parity with the Rust
application is in
`../../../../../docs/tek4051/web/design/graded-wasm-browser-parity-plan.typ`.

The generated module exports `BrowserEmulator`, `BrowserGranularity`,
`BrowserAutoLoadState`, `BrowserKeyMapping`, `BrowserImportSelection`,
`BrowserExportFile`, `BrowserArchiveFile`, `BrowserArchiveWriter`,
`BrowserDirectoryEntry`, `BrowserNotice`, `BrowserSnapshot`, and
`map_browser_key`. Pass `KeyboardEvent.key`, its optional paired single-character
text, and event-time Shift state to `map_browser_key_with_shift`; an unsupported
key or invalid paired text has no mapping. This preserves the intended
printable character when the host layout uses Shift differently from a US
keyboard. The two-argument `map_browser_key` remains available for callers that
use the conventional US Shift mapping. Forward the resulting `code` and
`invert_shift` to `key_down`, and call `set_running` and `advance()` from the
browser animation loop. Event pairing, key identity, focus handling, and
modifier collection remain browser tasks.

`advance()` uses the browser-compatible monotonic clock and returns an immutable
snapshot. The API also supports front-panel presses and releasing all input when
the browser surface loses focus; `has_held_key()` reports whether an emulated
key is currently owned by the session. `pending_input_transitions()` reports
queued sequenced keyboard transitions so hosts can coordinate input without
assuming a fixed key duration.

Snapshot `width`, `height`, `revision`, and status/register values are getters.
The `display_granularity` getter returns `BrowserGranularity`. `pixels()` returns
a copied row-major `Uint8Array` of green-channel intensities; compare
`revision` before requesting pixels to avoid copies when the display has not
changed. The wrapper exposes in-memory Flash Drive operations, not native
filesystem paths or dialogs.

`BrowserEmulator.queue_basic_program()` accepts numbered BASIC source lines,
submits each through the sequenced keyboard path after a fresh prompt, then
queues `RUN`. Hosts can observe `pending_input_transitions()` and wait for
`basic_prompt_ready` to confirm that the program has returned to BASIC. The
browser Tools > Editor tab validates and runs numbered source through this API.
Run starts the machine if needed and waits for machine-detected BASIC readiness;
its completion status means the commands were sent, not that program execution
has finished. Existing lines with matching numbers are replaced, while other
machine program lines remain. Save prepares printable ASCII records terminated
by CR and a canonical Flash Drive `ASCII PROG` filename using an unused file
number. Loading a complete Flash Drive `.bas` filename displays only its
description in the Filename field. Save selects an output folder and writes the
generated filename there; replacing an existing file requires confirmation.
Browsers without folder-selection support report a save error rather than opening
a filename-edit dialog or silently downloading elsewhere. The Tools > Test tab also uses this API for its grid, snake-fill, and
concentric-circle programs; each begins with `PAGE`. The grid has visible
borders and eight evenly spaced interior lines on each axis. Its coordinates
are scaled by `0.1265` to fit the visible browser canvas. Its single
float-coordinate strokes are filtered when the browser scales the canvas down,
so a line may become thinner or dimmer instead of being dropped. The browser
regression samples a filtered copy at the displayed canvas size and checks all
ten line bands and all four borders.

The Tools > Test panel can enable optional vector-scene capture when Extended
WebGPU is active. Enabling capture resets the emulator to establish a fresh
scene boundary; disabling capture releases the retained scene without resetting
the machine. The main-menu Raster/Vector selector appears while capture is
enabled and starts in Raster mode. Vector mode rerasterizes retained geometry
through the current camera and preserves the age of phosphor glow; the camera
does not change Machine `ScreenCopy` pixels or snapshot data. All drawn vector
strokes remain in retained geometry across display refreshes; a transient-vector
event adds a decaying glow rather than assigning the stroke a refresh lifetime.

The display camera also supports pointer-centered wheel zoom from 0.25× to 8×,
left-button dragging to pan, and right-click to restore the fitted view.
Zooming above 1× enlarges geometry without enlarging stroke widths; zooming
below 1× continues to scale stroke widths down.
`BrowserEmulator.set_vector_zoom_mode()` selects the presentation source, and
`set_vector_zoom_camera()` updates its logical-display zoom and pan without
advancing emulation. Raster mode retains the existing CSS camera transform;
Vector mode applies the camera in Extended WebGPU and avoids applying the CSS
transform twice. CPU fallback remains bitmap-only.
Test programs begin at line 100 in steps of ten and replace existing BASIC
lines in that range.

= Building the WASM bindings

From the repository root, build the WebAssembly library without the default
native feature, then generate the browser JavaScript glue with a `wasm-bindgen`
CLI matching the locked crate version:

```sh
cargo build --locked --target wasm32-unknown-unknown \
  -p tek4051-emulator --no-default-features --features wasm --lib
wasm-bindgen --target web \
  --out-dir target/wasm/pkg \
  target/wasm32-unknown-unknown/debug/tek4051_emulator.wasm
```

This builds the development profile. For performance measurements or a
deployment build, use Cargo's optimized release profile and pass its artifact
to `wasm-bindgen` instead:

```sh
cargo build --locked --release --target wasm32-unknown-unknown \
  -p tek4051-emulator --no-default-features --features wasm --lib
wasm-bindgen --target web \
  --out-dir target/wasm/pkg \
  target/wasm32-unknown-unknown/release/tek4051_emulator.wasm
```

A detailed record of the measured WASM rendering and CPU optimizations,
including complexity tradeoffs and rejected experiments, is available in the
#link("../../../../../docs/tek4051/web/design/wasm-performance-optimization-report.typ")[WASM performance optimization report].

For an optional thin-LTO build with one codegen unit, set Cargo's release
profile overrides on the build command:

```sh
CARGO_PROFILE_RELEASE_LTO=thin CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
  cargo build --locked --release --target wasm32-unknown-unknown \
  -p tek4051-emulator --no-default-features --features wasm --lib
wasm-bindgen --target web \
  --out-dir target/wasm/pkg \
  target/wasm32-unknown-unknown/release/tek4051_emulator.wasm
```

= WebGPU display renderer

The browser shell initializes the `wgpu` vector renderer before creating a
2D-canvas context. Its WebGPU surface samples the emulation-owned offscreen
frame without copying that frame through JavaScript. If the browser API,
adapter, device, or surface is unavailable, the shell presents CPU reference
pixels and reports the fallback in the Display status. A presentation failure
also disables WebGPU and switches to the CPU canvas.

`BrowserSnapshot.webgpu_active` identifies the selected renderer and
`BrowserSnapshot.render_error` explains an initialization or presentation
fallback. `BrowserSnapshot.pixels()` still returns CPU reference pixels; the
browser shell avoids requesting them while WebGPU is active. Browser-side
synchronous GPU readback is not implemented yet, so a machine screen-copy
operation explicitly falls back to CPU rendering. Active-renderer snapshot and
screen-copy parity remains open.

= Flash Drive API

Add selected loose files and ZIP bytes to a `BrowserImportSelection`, then pass
it to `import_selected_files`; this consumes the selection, preserves order,
applies the shared ZIP limits, and publishes ordered results through snapshot
import notices. `prepare_selected_files()` runs the shared bounded ZIP decoder
and exposes ordered results with `result_is_error`, `result_error`, and
`result_file`; `add_prepared_file` and `add_prepared_error` let a host apply
worker-prepared results without decoding ZIPs again.

The browser shell checks each `File.size` against
`max_flash_archive_file_bytes()` before the worker reads its `ArrayBuffer`, and
records host read/size failures in their original selection positions.

Use `snapshot.directory_len` and `snapshot.directory_entry(index)` for sorted
metadata. `description_utf16()` preserves the exact UTF-16 units, including
unpaired surrogates; callers must not convert those units to replacement
characters and treat them as lossless metadata. `export_file(number)` returns a
`BrowserExportFile` with the canonical filename and exact bytes.

`export_flash_drive_archive()` returns the shared whole-store ZIP format
synchronously for direct API consumers. The browser shell uses
`begin_archive_export_snapshot()`, `archive_export_snapshot_file(index)`, and
`finish_archive_export_snapshot()` to freeze and transfer a consistent raw
store snapshot. `BrowserArchiveWriter` invokes the same Rust archive codec in
the worker; `publish_archive_export_notice` records the completed host result
through the shared session.

`replace_flash_drive_archive(confirmed, bytes)` leaves storage untouched when
`confirmed` is false and transactionally replaces it when true. The returned
`Option` is empty for a declined replacement; parse/validation failures are
returned as errors and also published as import notices.

`auto_load()`, `set_basic_prompt_ready(ready, request)`, and
`launch_file(number)` use the shared session sequencer and launch gates. The
shared core detects readiness from the ROM's ordinary transient cursor glyph
or the transient question-mark glyph at a program INPUT prompt; hosts cannot
assert readiness independently. The compatibility setter can
clear readiness or attach legacy request metadata after detection. Snapshot
getters expose readiness, ignored-input notices, Auto Load state, launch state,
and operation notices.

Directory launch supports `ASCII PROG` and `BINARY PROG`. It types
`FIND@5:n`, then `OLD@5:` for ASCII or `CALL "BOLD",5` for binary, and waits
for a fresh BASIC-readiness signal after each completed command line before
sending the next. Cursor marks collected while a scripted line is still being
typed are discarded, so they cannot advance the sequence. Command delivery
does not guarantee that the program can execute: it may depend on a different
405x model (for example, a 4054) or ROM functionality not implemented by the
emulator.

The firmware redraws the Write-Through cursor during its input-wait loop and
pauses between redraws. Readiness follows the emitted glyph rather than
raster visibility or phosphor decay, so it remains true during the cursor's
off phase until an accepted Return, a stored/vector/erase display event, or a
run/reset/automation transition invalidates it.

= Browser shell

The package-local browser shell is in `platforms/wasm/browser/`. After building
the WASM library and generating its JavaScript bindings with the commands above,
serve the repository root so the shell can load output from `target/`:

```sh
python3 -m http.server 8000 --bind 127.0.0.1 --directory .
```

Open
`http://localhost:8000/dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser/`.
The page uses a title bar and menu bar above a two-column workspace. The menu
bar contains the Start/Pause toggle, Reset, and CPU speed selector; the title
bar has no separate paused or CPU-speed status labels. The display and
horizontal BUSY, BREAK, I/O, and POWER lamp bar occupy the left column. The
Tools panel is on the right, collapsed initially, and contains Flash Drive,
Editor, Controls, Workload, and Test tabs. The Flash Drive tab is selected by
default.
The workload tab reports recent emulation time as a percentage of one 10 ms
budget and preserves the native graph's average, peak, threshold, and gap
handling; its vertical time axis places the newest sample at the top. On narrow screens,
expanding the panel opens a right-side overlay while keeping the display column
in place. The Controls tab contains the existing display granularity, Caps
Lock, keyboard focus, and front-panel controls. The Start/Pause toggle exposes
its running state with `aria-pressed`. The display canvas preserves its native
aspect ratio while fitting the available display area as the browser viewport
resizes, without scrolling the shell. The renderer's 1046 × 830 framebuffer
remains distinct from the 1024 × 780 graphics area and 72 × 35 text grid. BUSY,
BREAK, and I/O lamps follow the keyboard PIA indicators; POWER follows the
running state. The BASIC indicator stays muted until the running session's
shared readiness signal confirms a BASIC prompt, then turns green; this is
status feedback and does not gate host keyboard input. The shell releases held
input when focus or document visibility is lost.

The shell also supports multi-file and ordinary ZIP import, separate confirmed
whole-store archive restore and confirmed Flash Drive clearing, canonical numbered-file and archive downloads,
directory selection with Up/Down and Enter/double-click launch, Auto Load, and
automatic BASIC cursor and program INPUT-prompt readiness detection. ZIP `File.size` is checked before reading;
file reads, selected-ZIP decoding, and archive encoding run in a stateless
Rust/WASM worker that never owns an emulator session. Imports are applied to the
main-thread session only after worker preparation succeeds, preserving ordered
per-file errors. Whole-store snapshots are frozen in the session and
transferred in 1 MiB chunks between animation frames; restore remains
transactional through the shared session API. Temporary download object URLs
are revoked on the next animation frame and on page exit.

The shared core gates host presses until the ROM emits either its ordinary
transient cursor glyph or the transient question-mark glyph at a program INPUT
prompt; ignored presses are reported in the shell. Readiness is cleared when
input is submitted or an operation starts, and is restored when a later prompt
glyph is drawn. The generated bindings under `target/wasm/pkg/` are build
output ignored by the workspace `.gitignore`.

= Browser tests

The Playwright suite contains DOM-only input-adapter tests and browser-shell
Flash Drive and BASIC editor tests that load the generated WebAssembly module. Build the WASM
bindings as above, restore npm dependencies if needed, then run from the
repository root:

```sh
npm ci
./node_modules/.bin/playwright test \
  --config dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser-tests/playwright.config.mjs
```

The DOM tests use a small session stub to isolate browser event ownership. The
Flash Drive tests exercise the real generated bindings, including import order,
pre-read ZIP size rejection, worker-backed selected-ZIP decoding and archive
encoding, confirmed archive restore/cancellation, canonical downloads, launch
gating, and animation-frame responsiveness for 64 MiB selected imports and
exports plus a 56 MiB whole-store restore. The Rust `wasm_api` tests separately
exercise the public session API. Browser keyboard taps are queued through the
timed input sequencer so rapid keydown and keyup events remain visible to the
emulated keyboard. Printable keydowns wait for paired browser text before
falling back to the logical key on key-up or a subsequent keydown, preserving
layout-dependent characters when the text event arrives later.

= Optional local performance benchmark

The opt-in Playwright benchmark measures the running WASM shell at all seven
CPU speed settings. It reports a live BASIC prompt/cursor workload and a BASIC
vector-drawing loop, using a three-second rolling window at each speed. Results
are host-dependent and are reported without pass/fail performance thresholds.
The benchmark requires the generated WASM bindings and installed Playwright
Chromium. From the package-local `platforms/wasm/browser-tests/` directory, run:

```sh
RUN_WASM_PERF_BENCHMARKS=1 npx playwright test \
  -c playwright.config.mjs performance.spec.mjs
```

The report is printed by Playwright and attached under the Git-ignored
`target/browser-input-tests/` output directory. It records browser and viewport
details, `emulator.advance()` wall time as a percentage of one 10 ms budget,
whole animation-frame intervals, skipped unchanged-revision presentations,
and synchronous `present_webgpu()` wall time. When the browser supports
instrumentation, it also reports WebGPU canvas texture-acquisition time.
Advance time is not total browser CPU utilization. While BASIC is at its
prompt, the benchmark also sends one printable key at x1, x20, x50, and x100.
It reports the Playwright call wall time and any browser Event Timing
input-delay entries (`processingStart - startTime`) with duration of at least
16 ms, plus a page-to-key-handler delay measured from a timestamp immediately
before Playwright sends the key. These timings describe browser event
dispatch, not keyboard-PIA delivery or BASIC echo.

Use `--headed` to exercise browser WebGPU when the host supports it; set
`WASM_FORCE_CPU_RENDERER=1` to compare against the CPU fallback in the same
headed browser environment.

Run the API tests on the host without native UI dependencies:

```sh
cargo test --locked -p tek4051-emulator --no-default-features \
  --features wasm --test wasm_api
```
