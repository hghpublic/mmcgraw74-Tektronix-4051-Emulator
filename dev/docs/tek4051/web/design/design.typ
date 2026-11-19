#import "@preview/merman:0.6.0": mermaid-figure

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
  #text(size: 15pt)[Rust and egui Port Design]
]

#align(center)[
  Review basis: `dev/src/4051-Emulator/web/4051-Emulator`
  \
  Rust implementation root: `dev/src/4051-Emulator/rs/4051-Emulator-rs`
  \
  Cargo workspace: repository-root `Cargo.toml`
  \
  Status: partial native implementation; parity baseline and grade gates remain open
  \
  Updated: 15 November 2026
]

#outline(title: [Contents], depth: 2)

= Purpose and scope

This design records the web emulator's boundaries, the current Rust/egui implementation, and the remaining target architecture. The web implementation remains the compatibility reference. Isolated comparisons exist, but they do not establish complete parity.

The selected target is a native egui application using eframe. The repository-root `Cargo.toml` is a virtual workspace; its member package manifest, library, and native binary live under `dev/src/4051-Emulator/rs/4051-Emulator-rs`. Dependencies include eframe 0.32.3 and Jiff for RTC local-time handling.

The user selected storage behavior matching the reference's in-memory tape store. The draft baseline observed saved tape files disappearing on browser reload, despite the README's browser-local-storage claim. Revisit the selected policy only if contrary runtime evidence emerges.

The UI-independent Rust core implements a subset of MC6800 instructions, memory mapping, PIA behavior, interrupts, keyboard signals, display modeling, host timing, and a GPIB handshake with a program Loader, real-time clock, and partial tape device. A native eframe shell presents the display and a subset of controls. Bounded machine/runtime tests run a BASIC program entered by keyboard or supplied through the Loader or in-memory tape API; full boot, complete CPU/board behavior, native input, and feature parity remain unverified.

The compatibility target is the behavior actually established by the reference implementation and its supplied examples, including its documented limitations. It is not a claim to emulate every Tektronix 4051 peripheral or every feature of later 4050-series models.

= Reviewed system

== Entry point and runtime composition

`Tektronix4051.html` is the application entry point. It loads the CPU, base ROM, display, keyboard, machine, storage, ZIP, ROM expansion, RTC, loader, GPIB, and printer scripts directly, then constructs default GPIB devices at addresses 1, 2, and 5. The page owns the canvas, keyboard controls, status indicators, speed and pixel-mode buttons, storage dialog, and GPIB dialog (`Tektronix4051.html:8-22, 37-58, 186-192, 300-318`).

The page's JavaScript also connects start, pause, reset, auto-scaling, CPU speed, display granularity, file loading, audio, and device-registration actions to the emulator (`Tektronix4051.html:364-404, 465-546, 605-621`). The stylesheet contains the fixed front-panel layout and screen presentation (`styleSheet.css:1-160`).

== Logical responsibilities in the web implementation

#table(
  columns: (1.1fr, 2.2fr, 1.7fr),
  stroke: 0.5pt,
  [*Area*], [*Existing responsibility*], [*Source evidence*],
  [Machine and board], [Owns CPU, RAM, base and expansion ROM access, PIAs, keyboard signals, display control, interrupt state, and GPIB handshake between the 6800 and peripherals.], [#raw("TEKTRONIX4051.js:3-25, 452-489, 1126-1209")],
  [CPU], [MC6800 registers, instruction execution, reset vector and speed-factor adjustment.], [#raw("mc6800.js:54-68, 326-331, 2396-2412")],
  [Display], [Converts board display signals into persistent vector and character pixels; supports phosphor decay, erase, copy, and three pixel-size modes.], [#raw("TekDisplay.js:3-19, 63-145, 159-241, 272-365")],
  [Keyboard], [Maps host keys and emulated front-panel keys to Tektronix codes and shift/control/caps-lock signals.], [#raw("TekKeyboard.js:11-93, 279-360")],
  [GPIB devices], [Routes addressed device commands and buffers bytes; supplied devices are Loader, RTC, Storage, and Printer.], [#raw("GPIB.js:3-36, 143-177; Loader.js:1-29; RTC.js:1-48; Printer.js:1-44")],
  [Tape storage], [Models numbered files and metadata; provides GPIB tape operations and browser import/export flows.], [#raw("Storage.js:38-65, 200-541, 770-1050, 1076-1428")],
  [ROM data], [Contains base system ROM arrays and expanded ROM slot data; these bytes are executable compatibility assets, not generated code.], [#raw("Tek4051Rom.js:1-3, 5960-5982; RomExp.js:10-13, 105-1137")],
)

The web page is the only composition root. The JavaScript modules use browser objects and direct global references, so their current boundaries are useful responsibility clues but are not clean Rust module contracts. In particular, keep the 6800's memory-mapped I/O and GPIB bus sequencing in the machine layer rather than moving hardware behavior into egui callbacks.

== Runtime and build observations

The inherited web source directory has a `package.json` with ESLint development dependencies and a placeholder `npm test` command. The repository-root `npm test` instead runs the newer Playwright workspaces for reference workflows; their results and skips are reference evidence, not proof of Rust parity. The HTML loads a classic-script `RTC.js`; a separate `RTC.mjs` is present but is not in that script list. Likewise, `.old` and `.new` files in the folder are not loaded by the application entry point. Establish the canonical source from the HTML dependency list; do not automatically port duplicate or unused files.

`Storage.js` uses a `fileStore` array initialized with each `Storage` object (`Storage.js:38-40, 1076-1210`). The source README says browser-local storage was added (`README.md:22`), but the reviewed storage implementation contains no `localStorage`, `sessionStorage`, or IndexedDB calls. The draft baseline observed that saved files disappear on page reload; native storage has not yet been implemented.

= Behavior and compatibility surface

== Machine execution

The root project README describes a Motorola 6800, 32 KB RAM, 32 KB BASIC ROM, and 1024 by 780 vector graphics area, together with the supported 4051 option-ROM set (`README.md:109-127`). The machine implementation routes memory reads and writes through RAM, ROM, PIAs, and the GPIB-facing board logic (`TEKTRONIX4051.js:489-803`). The CPU executes batches on a 10 ms interval, while display phosphor decay runs at 100 ms; the speed factor changes the CPU's instructions-per-interval (`TEKTRONIX4051.js:1201-1209, 1310-1318; mc6800.js:60-66, 326-331, 2409-2412`). Keep instruction execution deterministic and independently testable; application emulation scheduling requests bounded batches independently of egui repaints. CPU execution must not run inside the GUI's `update`.

== Screen and keyboard

The logical display is not a normal text terminal. It plots vectors and character dots from PIA state, retains pixels above a brightness threshold, and erases pixels during refresh (`TekDisplay.js:63-145, 159-241, 307-329`). Preserve coordinate direction, offsets, pixel intensity, and coarse/mixed/fine dot sizing before optimizing the renderer. The current HTML canvas is 1046 by 830, while the user-facing project description specifies the 1024 by 780 graphics space; keep the logical plotting area distinct from the surrounding front-panel UI (`Tektronix4051.html:58; README.md:109-113`). The Rust `DisplayModel` keeps the reference-sized canvas as row-major green-channel intensity values, with PIA reads/writes driving vector, cursor, and character-dot output. `HostRuntime` applies the 100 ms phosphor update and completes the 500 ms erase transition; the native shell uploads framebuffer revisions as an egui texture.

The native display texture uses linear filtering for enlargement and reduction instead of nearest-neighbor sampling. This is a presentation-only change: logical dots, phosphor thresholds, and full-resolution PNG copies are unchanged. A matched-size native/browser character comparison remains required to verify visual quality; linear filtering does not establish browser rendering parity.

The host-key mapping includes ordinary characters, modifier state, Tektronix function keys, HOME/PAGE, RUB OUT, BREAK, and other special keys. Front-panel keys produce press and release events, and the Auto Load control types a command sequence into the emulated keyboard (`TekKeyboard.js:11-93, 279-365; TEKTRONIX4051.js:1213-1275`). Treat both physical-key mapping and clickable front-panel input as required interaction surfaces.

== GPIB and storage

The default devices are the program loader at address 1, real-time clock at address 2, and 4924 tape drive at address 5 (`Tektronix4051.html:304-318`). The GPIB class handles device registration, address decoding, input/output buffering, and device dispatch (`GPIB.js:3-18, 54-177`). The RTC returns local date and time with English month names and milliseconds; the year modulo 100 is not zero-padded (`RTC.js:11-39`).

The tape UI exposes file number, type, usage, description, size, selected drive, viewer, save/delete/select operations, and storage clearing (`Tektronix4051.html:204-257`). Import/export has three distinct formats: a single file, a storage archive, and a collection of 4050 Flash Drive files. Archive import/export uses JSZip; Flash Drive filenames encode file metadata; drag/drop is also supported (`Storage.js:200-230, 265-395, 420-541, 1333-1428`). GPIB tape commands include save, old/load, directory, print, input, read, write, binary load/save, list, and find (`Storage.js:51-65, 770-1050`). Keep byte-level GPIB behavior and file-format conversion separate from UI dialogs.

== Documented limitations

The project README explicitly says R12 MUSIC and SOUND commands do not play on the emulator (`README.md:86`). Preserve that as the current compatibility boundary; adding sound synthesis would be a separate feature, not a parity requirement. The UI's beep/click audio toggle is separate from BASIC's R12 SOUND/MUSIC behavior (`TEKTRONIX4051.js:153-157, 914, 1145-1166`).

= Rust/egui architecture

== Agreed boundary direction: first discussion iteration

On 15 November 2026, the user agreed to the following architectural direction. The initial discussion did not authorize source changes; the user subsequently requested starting refactoring. The first bounded AR-1 increment now groups machine code, as recorded below and in the refactoring guide; the remaining boundaries are still work to do. The central boundary separates the simulated computer from the application operating it. Grouping files must reinforce responsibilities and dependencies, not merely make the directory look tidier.

Architectural refactoring is tracked independently in `graded-architectural-refactoring-guide.typ`, using grades AR-0 through AR-5. Its gates preserve existing Rust behavior while establishing boundaries; they neither require completed migration grades nor establish web feature parity. `graded-migration-guide.typ` continues to govern migration acceptance.

=== Responsibility groups and dependency direction

Use two top-level responsibility areas: *machine* and *application*. Within application, keep *GUI* and *emulation* distinct. The conceptual dependency direction is:

```text
application
    GUI -> emulation -> machine
```

The machine must not depend on either application layer. Emulation must be usable and testable without a GUI. The first increment uses a `machine` module within the existing package; it does not settle the eventual application layout, require separate Cargo packages or prescribe one file per listed responsibility.

#mermaid-figure(
  ```mermaid
  flowchart TD
    subgraph ArchApplication["Application"]
      subgraph ArchGui["GUI"]
        ArchViews["Views"]
        ArchHostInput["Host keyboard"]
        ArchHostIo["Dialogs and host I/O"]
      end
      subgraph ArchEmulation["Emulation"]
        ArchSession["Session and commands"]
        ArchPacing["Pacing and macros"]
      end
    end
    subgraph ArchMachine["Machine"]
      ArchBoard["CPU, bus, memory, PIAs, ROMs"]
      ArchDevices["Tube, keyboard and device I/O"]
    end
    ArchViews -->|"controls"| ArchEmulation
    ArchHostInput -->|"Tek key input"| ArchEmulation
    ArchHostIo -->|"import and export"| ArchEmulation
    ArchSession -->|"uses"| ArchPacing
    ArchSession -->|"owns and operates"| ArchMachine
  ```.text,
  document-context: true,
  width: 100%,
  error-mode: "panic",
  caption: [Agreed responsibility groups; the machine module is grouped, while application layout and crate boundaries remain open.],
  alt: "The application contains GUI and emulation layers. Views, host keyboard translation and host I/O submit requests to the emulation session. The session uses pacing and macros, and owns and operates the machine. The machine groups CPU, bus, memory, PIAs, ROMs, tube, machine keyboard and device I/O.",
) <architecture-layers>

Arrows show application dependencies and requests, not electrical signal wiring. The keyboard inside *Machine* is the Tektronix interface, distinct from host-key translation in *GUI*. State and display snapshots return through emulation; that return flow is omitted to keep the dependency direction clear. The boxes group responsibilities without committing to additional files or wrapper types.

#table(
  columns: (1fr, 3.8fr),
  stroke: 0.5pt,
  [*Area*], [*Owns*],
  [Machine], [CPU, bus and memory, PIAs, base and expansion ROMs (images under `machine/roms/`), tube/display model, machine keyboard interface, machine-level I/O and devices.],
  [Application: emulation], [Mutable machine ownership, execution scheduling, pause/reset/speed policy, worker communication, scripted input and program launch. The supported CPU speed multipliers are defined once here for both session validation and GUI selection.],
  [Application: GUI], [Windows, widgets, display presentation, host keyboard/pointer interpretation, native dialogs and user-facing feedback.],
)

The GUI submits operations through emulation rather than mutating CPU, PIA or device state directly. Keep one clear mutable owner of the machine. Reuse its existing interface where suitable; a module boundary does not require another object forwarding every machine method.

=== Machine behavior versus host/application policy

The tube/display model owns dots, vectors, phosphor behavior and erase state. Textures, scaling, window layout and image export belong to the application. Device behavior must remain testable from supplied inputs and time; wall-clock pacing, speed selection, pause and overload coalescing belong to emulation.

Machine-level I/O includes memory-mapped accesses, GPIB sequencing, device transfers and stored device contents. Application-level I/O includes host paths, file choosers, host reads/writes and import/export orchestration. Device protocol and state must not leak into dialog callbacks, and host filesystem operations must not become hardware behavior. Shared format codecs live under `flash_drive/`, and host-clock sampling lives under `host/clock.rs` behind its existing public compatibility path; neither placement justifies speculative adapter layers.

User-level keyboard input is separate from the machine keyboard interface:

```text
Host event
    -> application input translation
    -> emulation command
    -> machine keyboard interface
    -> PIA
```

The application interprets host layout, text events, shortcuts, focus and widget ownership, including German macOS Option+L. The machine understands Tektronix key codes, modifier lines, keyboard state and interrupt behavior; it must not know about egui focus or directory checkboxes. Scripted FIND/OLD/RUN typing is application emulation behavior, not machine hardware behavior.

Logical responsibility is distinct from execution thread. Expensive display conversion may execute on a background worker to preserve responsiveness without making GUI-specific image types part of the intended emulation model. Moving work out of a repaint does not by itself establish the desired module boundary.

=== Immediate value and limits on complexity

Every additional boundary or abstraction must have immediate, identifiable value for modularity, maintainability, testability or dependency control. Avoid accidental complexity:

- Prefer concrete types and ordinary functions over speculative traits, plugin systems or generic frameworks.
- Introduce boundary types only when they remove an actual dependency or ambiguity.
- Separate responsibilities, not every small operation; keep closely related code together rather than scattering it across generic services, managers or utilities.
- Keep machine and emulation behavior testable without widget details, and test host-key translation separately from PIA behavior.
- Preserve existing emulator behavior, compatibility assets and GUI responsiveness while discussing or later changing structure.

A separate machine crate may justify itself through compiler-enforced dependencies and builds/tests that do not compile the GUI stack. That decision remains open; a crate per device is not the starting point. Agree on ownership and allowed dependencies before settling the physical file layout.

=== Gap between this direction and the current implementation

The native worker separates execution ownership from GUI repainting, and AR-2 separates input sequencing and transport from host interpretation. `emulation_input.rs` owns ordered neutral input frames, accepted key identities, modifiers and timed taps/macros without egui types. The machine retains actual keyboard/PIA state. `gui/keyboard_input.rs` translates native events; `gui/front_panel.rs` draws keys and resolves clipped hit regions. `gui/app.rs` owns native widget presentation and schedules existing host jobs.

`emulation/session.rs` receives only neutral input operations and does not own native input/view adapters. The GUI does not predict accepted ownership: stable opaque key IDs let the worker ignore modal input and still release the last accepted key correctly. Focus cleanup, empty-frame coalescing, reset transitions and ordered releases retain regression coverage.

The first AR-3 increment also removes GUI image types from `emulation.rs`. Its snapshots share immutable grayscale pixels by display revision, independently of retained event-time screen copies. `native_backend.rs` pairs each snapshot with a cached green egui image before publication. Conversion remains on the native worker, and repaint uploads that prepared allocation without recoloring pixels.

The neutral view adds a grayscale allocation for each live revision; unchanged metadata publications reuse both raw and native images. This concrete representation cost buys an independently testable session/snapshot boundary without a generic publisher or forwarding facade. The desktop backend remains deliberately presentation-specific.

The next increment removes binary-root file-helper calls from the session. File commands emit concrete `FileEffect` values; `host/io.rs` executes them on the native owner and returns `FileOutcome` notices to the session. Numbered exports and screen saves own selected data; archive streaming uses the same suspended machine. The worker completes each effect before draining another command or advancing time, preserving modal/store ordering without an extra queue or whole-store copy.

The next increment makes production session data private. Native transport uses explicit advancement, file-time suspension/completion, failure and shutdown operations rather than editing runtime/input fields. Host I/O receives archive streams through the session, not a mutable machine reference. Existing business methods operate on the original data in a private container; no additional session/controller layer is introduced.

The earlier private-state adapter gave the session a test-only field `Deref`, which could conceal invalid normal-build accesses. That session `Deref` is now removed. Private native fixtures instead live in a test-only descendant module of the library session and access its data explicitly, or through the private widget fixture adapter. No production field visibility is widened for those tests.

The library owns `Command`, `Snapshot`, `FileEffect`, `FileOutcome` and the public `EmulationState` in `tek4051_emulator::emulation`. The existing session implementation is now in `emulation/session.rs`; native transport and host effects consume it directly rather than compiling another copy. The session exposes command application, snapshots, time/lifecycle operations and archive streams, but no mutable machine/runtime/input reference. It creates no worker or window and does not execute returned filesystem effects.

`main.rs` is a thin executable entry point loading `native/app.rs`. That native composition module retains startup, helper adapters and event-loop shutdown, while `native/backend.rs` owns bounded worker transport and image preparation. With the default `native` feature, the same app source is compiled under the library session's test-only scope for private regressions; production library code does not include those widgets/adapters. These fixtures compile but do not invoke native startup, with a narrow expected-dead-code annotation on that startup function.

With default features, all 100 native regressions run exactly once in the library target alongside eleven core/codec unit tests. Three package integration tests use only the public session/machine API to verify real CPU/erase/capture/error/pause behavior, prepared-machine and nondecreasing pause/resume clock boundaries, and transactional archive replacement with exact old/new bytes. Test-only dialog/export/speed helpers live separately in `native/test_support.rs`. The relevant command is `cargo test --locked -p tek4051-emulator --lib --test emulation --quiet`; the thin binary unit target no longer contains the native suite.

The host prepares the machine before session construction (reset for ROM boot or explicitly configure a custom program). The constructor does not reset registers. Hosts supply nondecreasing timestamps and advance through the old Run/Pause state before changing it; command application does not account for that earlier interval itself. These caller responsibilities preserve existing timing semantics rather than adding another clock/controller.

The default-enabled `native` Cargo feature now owns optional `eframe`, `rfd` and PNG `image` dependencies; the executable requires that feature. Normal build/run behavior remains native. `cargo build --locked -p tek4051-emulator --no-default-features --lib` selects a GUI-free library build. The native private fixtures and self-crate test alias are also excluded in that configuration; its eleven core/codec units and three public session tests pass without those adapters.

An offline build/test/strict-Clippy run in a fresh workspace-local target directory and an explicit transitive normal/build/dev dependency check establish this package-scoped boundary. No egui/eframe, window/rendering, dialog or image dependency enters that graph. Existing core time/archive dependencies (`jiff`, `zip`) remain. Downstream headless consumers must select `default-features = false`; Cargo features are additive, so another selected consumer can reenable `native`. This is not a GUI-free whole-workspace claim: the shared Flash Drive test package retains native dependencies and its default-enabled emulator dependency.

The opt-in `wasm` feature exposes `platforms/wasm/mod.rs` as a `wasm-bindgen` browser-session API and builds the library as both `rlib` and `cdylib`. Use `--no-default-features --features wasm` for `wasm32-unknown-unknown`; this feature also enables the shared `wgpu` vector renderer. Target-specific `web-time` supplies the monotonic session clock and Jiff's `js` feature enables browser RTC sampling. The browser collects DOM events and owns canvas/surface presentation; it prefers WebGPU and keeps the CPU reference renderer as an observable fallback. The shared Rust keyboard mapper translates logical keys and optional paired text. The adapter exposes lifecycle/input controls and immutable status snapshots, not native dialogs or Flash Drive host effects. Snapshot metadata shares the session's `Arc`; `BrowserSnapshot.pixels()` returns the CPU reference frame, so the shell avoids requesting it while WebGPU is active. Compare `revision` before requesting CPU pixels. The staged path to keyboard and Flash Drive browser parity is in `graded-wasm-browser-parity-plan.typ`; it is a separate track from the web-to-Rust migration grades.

Browser-shell-only WebGPU presentation optimization, with Rust emulator sources and manifests out of scope, is tracked separately in #link("./graded-wasm-webgpu-optimization-plan.typ")[the graded WASM WebGPU rendering improvement plan]. Optional Extended-WebGPU-first vector zoom, including its feature gate, reset boundary, camera controls, and Machine/ScreenCopy constraints, is specified in #link("./vector-zoom-implementation-plan.typ")[the vector zoom implementation plan]. Separately authorized renderer and shared-CPU performance results, including the boundary between those efforts, are documented in #link("./wasm-performance-optimization-report.typ")[the WASM performance optimization report]; they do not expand the browser-only plan's scope.

The crate-private `flash_drive/archive_json.rs` owns the archive header's JSON syntax and lossless UTF-16 string encoding, independent of tape state, ZIP or host effects. Its parser/encoder algorithms and diagnostics are retained. Two private tests in `flash_drive/archive_json/tests.rs` exercise exact escapes, raw/escaped Unicode pairs, isolated surrogates, malformed strings and trailing data without constructing a machine.

The shared `flash_drive/mod.rs` owns archive header schema validation/encoding and common file-type/usage codes and labels. GPIB/tape consumers use those same definitions for device metadata; there are no duplicate code tables. The archive encoder borrows description units rather than accepting a private tape object or copying file contents. Its original two private schema tests cover file-number boundaries, the 16-unit UTF-16 limit, independent type/usage combinations and explicit invalid metadata/export failures.

Fixed-width metadata formatting/parsing, the reference-style integer parser and host filename safety now also live in that shared codec. LIST and host names use the same UTF-16 formatter; only host names require valid Unicode and reject slash, backslash and NUL. The parser preserves positional type/usage letters, ignored size text, bracket removal, truncation/padding and out-of-range numeric headers with no replacement number. The original public export error is reexported at its unchanged GPIB path. Two additional private tests independently check exact Unicode/padding, raw LIST units versus unsafe host names and legacy marker parsing.

The shared `flash_drive/archive.rs` now owns whole-storage ZIP streaming, header/content pairing and resource limits. Export consumes an exact-sized iterator of borrowed metadata/payload views in caller order; tape supplies its sorted canonical store. Import validates all entries into owned metadata/payload records, ignoring entry names and retaining the last duplicate number. Tape moves those buffers into its private files before the unchanged atomic replacement/reset sequence. The intermediate map conversion may allocate metadata nodes, but does not clone payloads or descriptions. The original public limit paths under `machine::gpib` reexport the codec-owned constants.

These shared interchange utilities are not application adapters; hardware remains independent of GUI/emulation orchestration. Tape no longer imports ZIP types or owns host filename policy. LIST low-byte/sentinel handling, retained-number/usage updates, canonical lookup, batch import, storage and transfer-state policy remain in tape; existing stream APIs and the private file-to-metadata adapter delegate to shared codecs.

RTC sampling now uses a concrete function-pointer boundary. `host/clock.rs` implements public `host_clock::local_datetime`, which performs the original Jiff system-time and local-timezone lookup; `lib.rs` preserves the root module path with `#[path]`, and native startup explicitly installs the callback before machine reset. GPIB retains the calendar-to-byte formatter and samples only on address-2 command `0x6d`. `Machine::set_rtc_clock` supplies a callback for deterministic or alternative host values without changing buffered input or handshake state. Reset retains it. Errors retain the existing bus diagnostic, and the default controller still binds the same host-local sampler for compatibility with existing `Machine::default()` callers. The move changes physical file placement only; it is not a generic clock framework or removal of the machine's compatibility binding.

Two public PIA regressions verify an exact supplied date, reset retention, last-byte EOI and buffered-response stability after replacing the sampler, plus explicit injected failures only at read dispatch. Existing local-time-window, invalid-timezone and pure calendar-format tests remain. Default tests pass 111 library, three session and 66 GPIB cases; no-default offline passes eleven library, three session and 66 GPIB cases. Strict lint in both modes, native build and all-feature workspace consumer compilation pass.

The presentation adapter schedules file jobs; native rfd dialogs live in `host/dialogs.rs` and return only path choices or confirmation results. Selected loose files and bounded ZIP archives are read in `host/import.rs`, which returns owned Flash Drive inputs and per-file/archive errors without touching machine state. Effect writes live in `host/io.rs`. These adapters are physically grouped under `host/`, while `native/app.rs` keeps them as sibling modules to preserve private access. Presentation code and its private GUI tests now live physically under `gui/`; `native/app.rs` similarly keeps the app, panel and keyboard adapters as siblings so their existing access remains unchanged. This changes neither background-job nor worker ordering. Slow effect I/O still delays worker commands rather than GUI polling; physical native acceptance remains open.

== Current implementation

#table(
  columns: (1.7fr, 3.3fr),
  stroke: 0.5pt,
  [*Component*], [*Implemented responsibility*],
  [#raw("machine/cpu.rs")], [MC6800 registers, instruction subset, memory-bus access, and interrupt entry/return.],
  [#raw("machine/mod.rs")], [Declares hardware components, owns CPU and a private bus; samples interrupts and exposes stepping, keyboard, PIA and display operations.],
  [#raw("machine/bus.rs"), #raw("machine/pia.rs"), #raw("machine/rom.rs")], [Map RAM, base and selected option ROM payloads, PIA registers, keyboard and display signals, and board interrupt flags.],
  [#raw("machine/display.rs")], [Models the logical framebuffer, PIA-driven plotting, granularity, phosphor refresh, and erase transitions.],
  [#raw("machine/gpib.rs")], [Models GPIB input/output buffers, board handshake, the address-1 Loader, address-2 RTC, and partial address-5 tape.],
  [#raw("emulation/runtime.rs")], [Advances elapsed time through CPU, keyboard, and display callbacks; pauses and restarts timing at fresh interval boundaries. The root `host_runtime` path is a compatibility re-export.],
  [#raw("host/clock.rs")], [Default on-demand host-local calendar sampler, exported through the existing `host_clock` module path; explicit Jiff timezone/time errors, without RTC formatting or machine state.],
  [#raw("host/dialogs.rs")], [Runs native file/confirmation dialogs and returns paths or choices; does no filesystem I/O.],
  [#raw("host/import.rs")], [Reads selected loose files or bounded ZIP archives into owned inputs; no dialogs or machine-state mutation.],
  [#raw("emulation/input.rs")], [Applies ordered neutral input frames with accepted key identities, Shift/Control values, panel tap timing and FIND/OLD/RUN scripts using the machine keyboard interface. The root `emulation_input` path is a compatibility re-export.],
  [#raw("emulation/keyboard.rs")], [Maps framework-neutral logical keys and printable ASCII to Tek codes and Shift-line behavior without egui or DOM event ownership.],
  [#raw("flash_drive/archive_json.rs")], [Shared private JSON syntax/UTF-16 archive string codec; no machine state, ZIP, filesystem or GUI dependency.],
  [#raw("flash_drive/mod.rs")], [Shared metadata codes/labels, fixed-width/JSON headers and safe host filenames; no storage state or filesystem operations.],
  [#raw("flash_drive/archive.rs")], [Shared whole-storage ZIP streams, pairing and limits; borrows exports and returns owned decoded records without tape state.],
  [#raw("emulation/mod.rs")], [Library entry for shared command/snapshot/effect contracts and the reusable session API.],
  [#raw("emulation/session.rs")], [Privately owns machine/runtime/input data; exposes commands, timing/lifecycle, snapshots and archive streams, while retaining file-effect/notice policy.],
  [#raw("platforms/wasm/mod.rs")], [Optional wasm-bindgen browser API over the shared session: lifecycle, shared logical-key mapping, translated input, frame advancement and immutable display/status snapshots.],
  [#raw("native/backend.rs")], [Runs bounded native transport, prepares paired images and executes/completes file effects on the owner before further commands or advancement.],
  [#raw("host/io.rs")], [Executes archive/file/PNG effects via session streams and selected owned data, without obtaining mutable machine/runtime internals.],
  [#raw("main.rs"), #raw("native/app.rs"), #raw("gui/app.rs")], [Thin executable entry, native startup/file-job scheduling, and widgets that render snapshots and submit commands.],
  [#raw("gui/keyboard_input.rs"), #raw("gui/front_panel.rs")], [Translate native events/ownership and hit-test panel keys on the GUI before sending neutral operations. Neither adapter is owned by the worker.],
)

== Rust source organization

The following grouping records the current behavior-preserving organization. The emulation grouping, shared metadata/archive codecs, and physical host, GUI, and native file groupings are implemented.

#table(
  columns: (1.2fr, 3.8fr),
  stroke: 0.5pt,
  [*Group*], [*Responsibilities and placement*],
  [`machine/`], [Keep the existing hardware boundary, including GPIB/tape storage and transfer state in `machine/gpib/tape.rs`.],
  [`emulation/`], [The session, framework-neutral input sequencing, and elapsed-time runtime are grouped here. `HostRuntime` advances machine work from supplied elapsed time and is owned by the session; it does not access an operating-system clock, window, or thread.],
  [`host/`], [Group operating-system and filesystem adapters: clock sampling, dialogs, selected-file reads, and effect writes. `lib.rs` preserves the public `host_clock` module path for the sampler; native dialog/import/effect declarations remain under `native/app.rs` to preserve private sibling access.],
  [`gui/`], [Group egui widgets in `app.rs`, Flash Drive directory presentation in `flash_directory.rs`, front-panel presentation, native event/focus/pointer translation, and their private test adapters. `native/app.rs` keeps the app, panel, and keyboard modules as siblings via `#[path]`; `app.rs` owns the directory view as a child module. Send interpreted input to emulation as neutral operations.],
  [`flash_drive/`], [Shared, machine-independent metadata and archive codecs live here, including JSON, filename, and ZIP interchange rules. Keep device storage/transfer semantics in `machine/`, filesystem access in `host/`, and directory presentation and launch controls in `gui/`.],
  [`native/`], [Group native startup/composition, sibling declarations for GUI and host adapters, bounded worker transport, prepared-image publication, and their private test fixtures. Preserve module names and sibling visibility rather than introducing a forwarding layer.],
  [Integration bridge], [`native/backend.rs` combines worker transport with egui image preparation. Keep this cross-boundary role explicit; do not split it or add another abstraction solely for visual symmetry.],
)

The host dialog, import, and effect adapters are physically grouped under `host/`, while `native/app.rs` declares them as sibling modules using `#[path]`. This preserves their existing `pub(super)` visibility and consumer paths; nesting them under a new `host` module would require widening that visibility or adding wrappers. The clock implementation also lives under `host/clock.rs`, but `lib.rs` retains the public `host_clock` module path, preserving both `Machine::default()` and native startup bindings. The native app and backend files, plus their private fixtures, now share `native/` without changing their module names. These file moves add no runtime dispatch, allocation, copying, or synchronization; `native/backend.rs` remains an explicit integration bridge.

The GUI application, Flash Drive directory view, front-panel and keyboard adapters, inline test backend, and private GUI regressions are physically grouped under `gui/`. `native/app.rs` continues to declare the app, panel and keyboard adapter as direct sibling modules, and `gui/app.rs` owns `gui/flash_directory.rs` as a child. The app retains prompt/selection state and sends session commands; the directory view renders rows, maintains focus/navigation state passed to it, and returns a selection intent. The panel and keyboard tests remain separate private test files under their adapter directories. This preserves current `pub(super)` access and test ownership without changing runtime behavior or adding an indirection layer. The worker/image-preparation bridge lives under `native/backend.rs`.

The browser adapter is a feature-gated library module rather than part of `native/` or `gui/`. It translates JavaScript-facing calls into the same session commands and input frames; the shared keyboard mapper handles logical-key/character translation while the browser remains responsible for layout, event pairing, focus and drawing. Snapshots retain shared Rust pixels until JavaScript requests a copied typed array, and revision IDs let callers avoid unchanged-frame copies. The WASM build excludes the native feature and uses browser-compatible monotonic/RTC support.

The grouping changes where code is declared, not what it computes. The initial file moves and public-path `pub use` re-exports add no runtime dispatch, allocation, copy, or synchronization by themselves. Do not change execution budgets, framebuffer representation, worker scheduling, or file-processing behavior as part of a mechanical move. Added wrappers, dynamic dispatch, channels, copies, or a crate/dependency split would be separate design changes with their own costs and evidence requirements.

The main cost is source and test maintenance: imports and module declarations change, private test fixtures may need new descendant paths, and public module paths must remain compatible. In particular, keep the current `host_runtime` and `emulation_input` paths available while their implementations are grouped under `emulation/`.

Two seams need deliberate handling. `machine/gpib.rs` still binds `Machine::default()` to `crate::host_clock::local_datetime`; `lib.rs` maps that public compatibility module to `host/clock.rs`. This physical move leaves the dependency and default constructor behavior unchanged. Any future removal of the machine's host-bound default must be treated as a separate compatibility/interface decision. Likewise, keep `native/backend.rs` as a reviewed integration bridge unless a concrete dependency problem justifies separating worker transport from GUI image preparation.

The machine maps base and overflow ROMs, the backpack payloads, and all eight slots of each ROM expander. Selector 2/3 reads within the switchable window deliberately return zero for the reference's undefined bytes; unsupported selector-1 and selector-4 tails return errors. Board interrupt routing uses latched PIA flags as in the web reference. These choices do not establish full undefined-value or physical hardware parity.

The host runs CPU and keyboard callbacks at 10 ms, phosphor refresh at 100 ms, and an independent erase completion 500 ms after each request. Each CPU callback has an instruction budget derived from the selected speed. The native app starts paused, matching the web's explicit Start control; Start and Pause restart interval timing at fresh boundaries. Paused time does not run CPU, keyboard, or phosphor callbacks, but erase timeouts continue. Each erase completion blanks the screen and signals readiness, including a later completion after an earlier one has already signaled ready. Reset and execution errors do not cancel pending erase timeouts. Machine errors stop interval work and are shown in the UI; Reset resets CPU/interrupt state, starts another display erase flash, and can restart a stopped runtime while retaining the selected speed. Reset retains RAM and peripheral state. Invalid speed settings are reported without replacing the prior setting.

The Make Copy button and the DisplayX output-A bit `0x20` falling edge capture the current framebuffer. The machine preserves up to eight ordered snapshots and counts additional requests that cannot be queued; the app reports the lost copies without stopping emulation. The native app uses the selected Save As dialog and writes a full-resolution PNG; it reports save failures and cancellation. The modal dialog pauses interval work rather than adding dialog time to the next CPU update; elapsed dialog time still expires independent erase callbacks when the dialog closes. Unlike the web's automatic download, this requires a destination choice; native dialog behavior and overwrite confirmation remain unverified.

The GPIB buffers preserve the reference's input cursor on replacement, reset it on the final byte or explicit clear, and provide buffered and single-byte output. The machine's GPIB and keyboard PIAs drive the handshake; the Loader copies program bytes and supplies them at address 1 for secondary command `0x64`. A machine/runtime test loads `10 PRINT 1` via `OLD@1:` and executes it. At address 2, command `0x6d` samples host-local date and time using Jiff; timezone and clock failures are returned explicitly. The address-5 tape accepts numbered ASCII programs and Flash Drive file sets through the machine API and implements FIND, SAVE, OLD, CLOSE, PRINT, INPUT, READ, WRITE, BSAVE/BOLD, LIST read/write, and the root directory response. INPUT consumes one CR-terminated record from the selected ASCII snapshot; FIND of an existing ASCII/BINARY file rewinds the shared record cursor, while NEW/LAST markers retain snapshot/cursor state. EOF or an unterminated tail consumes remaining bytes without replacing shared input. Missing selections can retain earlier OLD bytes and cursor state. Machine/runtime tests cover Loader and tape program execution, ASCII/binary program saves and data round-trips, LIST headers, and a filename-imported ASCII program's FIND/OLD/RUN flow. Invalid headers/filenames, unsupported tape operations, missing required numbers, and FIND requests requiring direct-upload dialogs report errors. Native Flash Drive multi-file import, numbered-file export and running-only Auto Load are wired; other storage management, plain-file/ZIP import, archive export, printer, audio, and device configuration remain future work. The native app starts paused with Start/Pause, Reset, Caps Lock, display granularity, and speed controls. BUSY, BREAK, and I/O use keyboard PIA outputs; POWER reflects running state. Physical input and native presentation remain unverified.

Tape PRINT follows `Storage.js:820-846`: a zero record cursor replaces the current file's bytes; later writes append to the current file view and advance the shared cursor by the entire resulting buffer length. This does not refresh the separate FIND snapshot. SAVE changes stored bytes without updating the current file view, so a later PRINT can replace those saved bytes with an append to the earlier view. A cursor beyond the retained snapshot makes INPUT leave shared input unchanged. The numbered Rust subset requires a current file number, rather than creating the web's unnumbered entry from empty controls; cursor overflow is an explicit error. File types and usages are separate: ASCII/BINARY support PROG, DATA, LOG, and TEXT; NEW/LAST markers may retain a preceding usage. NEW PRINT/WRITE becomes ASCII/DATA or BINARY/DATA; LAST rejects both. ASCII/BINARY data transfers preserve LOG/TEXT, but BOLD requires BINARY/PROG.

Binary WRITE and READ follow `Storage.js:873-921`. WRITE uses the separate outbound byte on each DAV transfer and commits it immediately, independently of EOI, without advancing the shared record cursor. FIND of data arms clearing on the next selected WRITE; further bytes append to the current file view without updating the FIND snapshot. The web's private clear flag survives other FINDs and CLOSE; clearing occurs before the type guard, so an ignored ASCII WRITE can clear working contents without changing stored bytes. PRINT ignores binary files, WRITE ignores ASCII files, and INPUT/READ return `0xff` for incompatible types. READ supplies the whole current binary file only at cursor zero, sets the reference's consumed marker `99`, and otherwise leaves shared input unchanged. SAVE can replace the store with an ASCII program while READ still sees the prior binary working file until FIND. A ROM/runtime test round-trips the number 1 through WRITE/FIND/READ.

Binary-program command `0x71` follows `Storage.js:919-963`: the listener path stores the buffered BSAVE transfer at EOI as a binary program without updating the current selection or FIND snapshot. The talker path supplies BOLD only for a selected binary program; other kinds leave shared input unchanged. BOLD pads the snapshot with `0xff` followed by spaces to the next 256-byte boundary, always adding 1 through 256 bytes, and neither uses nor changes READ's cursor. WRITE preserves existing binary-program usage; program FIND does not arm the data-file clear flag. A real-ROM regression uses the mapped 4051R05 routines via `CALL "BSAVE",5` and `CALL "BOLD",5`, modifies the saved program, reloads it, and checks that RUN prints the original value. Tests cover exact empty/aligned/boundary padding and PIA byte/EOI ordering. Native export at file 0 and other R05 routines remain unverified or unimplemented.

LIST read (`0x73` with talker addressing) follows `Storage.js:969-995` and `tFile.name`: it returns the current marked file number padded to 7 columns, type to 8, usage to 5, a 16-column description, then a space and unpadded decimal byte size. NEW/LAST entries are valid without usage. Size/type come from the current working view, not a fresh store lookup; SAVE and an ignored ASCII WRITE can therefore leave different metadata until FIND. Store existence alone is insufficient when the current selection is untyped; missing/untyped/CLOSE states supply `0xff`. The response has no trailing CR or NUL, and shared input replacement retains its GPIB cursor. PIA tests check exact layouts, a 256-byte size, stale/current metadata, and sentinels; a ROM program uses DIM/INPUT/LEN to verify the 38-character entry for an ASCII program.

LIST write (`0x73` with listener addressing) commits at EOI using the `flashFilenameHandler` / `tFile.header` rules (`Storage.js:997-1004, 1303-1321, 1333-1428`). The parser reads only the first six bytes for a parseInt-style number, type at byte 7, and ASCII/BINARY usage at byte 15; remaining label letters and the final size value are not validated. Description bytes from byte 20 to the final space have brackets removed before 16-byte truncation/padding. Bytes remain JavaScript-compatible one-byte characters, not UTF-8. Out-of-range numeric numbers pass parsing but retain the preceding current number, as the tFile setter does; NEW/LAST preserve preceding usage. The header saves current working contents to the resulting key without deleting the old key or updating snapshot/cursor/pending WRITE clear. The separate FIND/fileNum control used by SAVE/BSAVE is unchanged, even after current-file renumbering. Malformed headers deliberately surface a typed Rust bus error instead of the web's silent rejection; an absent usable number remains an explicit error rather than creating an unnumbered entry. PIA tests cover these transitions, byte-level parser edge cases, LOG/TEXT transfer gates, and marker behavior. A real-ROM DIM/PRINT/FIND/INPUT program writes a header for file 2 and prints its complete metadata, matching an independently specified string literal.

== Existing native behavior and adapters

The optional native Flash Drive directory is a read-only side panel. The machine returns canonical metadata snapshots in ascending file-number order, mapping existing type/usage labels and copying only raw UTF-16 description units, not file contents. Byte counts come from the stored payload length. Selection, current/FIND views, cursors, buffers and pending WRITE clearing are unchanged. The panel refreshes while visible, trims only trailing description spaces for presentation and visibly reports invalid UTF-16; missing usage is permitted for NEW/LAST. It remains available paused/stopped or during Auto Load. Unlike a floating window, the side panel occupies a separate area below the controls. Core tests cover sorted metadata, SAVE/WRITE updates and retained transfer state; native tests exercise the real checkbox, live updates, unchanged framebuffer/registers and macro ownership, plus malformed descriptions and markers. Physical interaction remains unverified.

The checkbox owns Space/Enter while focused and on its lost-focus frame. The keyboard adapter retains each owned press through repeat/release and suppresses paired text, even after focus moves, without cancelling panel or Auto Load input. Before drawing widgets, it consumes owned activation repeats and paired text from egui's widget events, leaving raw events intact for ownership tracking; a held activation therefore cannot trigger a newly focused Reset button. Tab repeats remain available for native navigation. By the user's selected policy, Tab belongs to native navigation while the directory is open or a native control already has focus; otherwise it reaches BASIC with the existing manual-input cancellation behavior. The app retains prior-frame native focus to distinguish automatic first-widget Tab focus from an already-focused control, and surrenders the automatic focus when Tab belongs to BASIC. Other routing is unchanged, and number-editor focus still uses its separate full-keyboard guard. Real-widget regressions navigate with Tab during Auto Load, activate both checkbox keys, Shift-Tab to Reset after closing the directory, and check repeats/releases and resumed ordinary input. A hidden-directory/no-native-focus regression verifies BASIC Tab, macro cancellation and key cleanup.

Native Flash Drive import follows `importFlashDir`, sorted selection, and filename parsing (`Storage.js:116-130, 265-326, 646-664, 1333-1428`). The existing rfd integration opens a multi-file chooser; filesystem reads stay in the native shell. The core returns one typed result per readable file, keeps successful imports despite failures, replaces duplicate numbers in input order, and retains other stored files. Selection changes to the first number in the whole store without refreshing snapshot/cursor or arming/disarming WRITE clearing. Import neither types commands nor starts the machine. Invalid filenames, out-of-range numbers, and host read errors are reported instead of silent skips or accidental unnumbered entries. Native filenames require Unicode; descriptions are parsed and stored as UTF-16 units before GPIB low-byte conversion, preserving surrogate truncation and avoiding false type/bracket aliases. Header writes use the same parser with their original one-byte input units.

The user authorized native dialogs and partial imports with per-file errors. Native dialog/read time follows screen-copy timing: interval callbacks are excluded, independent erase callbacks catch up, and running/paused state is retained. Headless tests click the import control with injected dialogs, verify partial/zero-success errors and cancellation, read a real binary fixture, and check post-dialog CPU/erase timing. PIA tests cover metadata, duplicates, markers, retained transfer state, and Unicode filenames; a ROM test imports and runs a small program. Actual native chooser interaction and broader supplied-program imports remain unverified. Tape storage remains in memory; import does not modify original host files.

Native Auto Load queues `FIND@5:1`, `OLD@5:`, and `RUN`, each followed by CR, as the web control does (`TEKTRONIX4051.js:1250-1275`). It reuses the front-panel queue's 100 ms press/release intervals with fixed inactive Shift/Control instead of the web's asynchronous speed-dependent typing. The user selected running-only operation: no implicit Start, pause-time queue retention, file import, or tape-file validation. Busy manual input is reported; duplicate requests and import are disabled while typing. Pause, Reset, runtime errors, manual host/panel input, and focus loss cancel unsent keys. Focus loss is checked before elapsed macro catch-up, so a delayed unfocused frame cannot drain pending commands before cancellation. The UI distinguishes typing, keys sent, cancellation with possible partial editor input, and busy-key errors; sent keys do not imply program success. Tests observe the complete PIA key sequence and 99/100 ms boundaries, held-Shift isolation, interruption and replacement ownership, a six-second delayed unfocused frame, and the actual egui control's real-ROM output mask against independently typed BASIC. Core drawing/phosphor, tape, and interval scheduling algorithms are unchanged. Actual native interactions and broader supplied programs remain unverified.

Native numbered-file export copies the canonical tape entry through a read-only machine API, not the stale working file or FIND snapshot. LIST and export share the 7/8/5/16 metadata formatter, retaining LIST's low-byte output while native names require lossless Unicode and reject slash, backslash and NUL. Export recomputes byte size, returns typed number/missing-file/filename errors and does not change selection, buffers, cursors or pending WRITE clearing. Native Save As writes raw bytes with visible success/cancel/error notices; export is disabled during Auto Load typing and otherwise remains available when paused or stopped. Dialog/write time excludes CPU/keyboard/refresh intervals while erase callbacks catch up. Public tests cover metadata/byte round trips and SAVE-versus-snapshot behavior; private tests cover real controls, file readback/errors and modal/Auto Load timing. Marker usage is still ignored by filename import. Actual native chooser/overwrite interaction and archive export remain unverified or future work.

The export-number editor is also disabled during Auto Load typing. Its focus and focus-loss/commit frame suppress host key forwarding, release held emulator keys and modifiers, and cancel pending panel input without discarding new panel pointer events. The editor's digits and Enter therefore stay out of BASIC. A native UI regression checks held Shift/Control/A release, number entry, Enter isolation and subsequent HOME pointer delivery; exact canonical filename expectations and a distinct-file export before SAVE cover metadata and target preservation.

The machine components now share one Rust module with a private bus; `host_runtime` remains separate. Native presentation, front-panel and keyboard adapters now live under `gui/`, and session/input/runtime implementations live under `emulation/`, with existing native module relationships preserved. Host-clock sampling is implemented in `host/clock.rs` and exposed through the unchanged public `host_clock` module path; GPIB formats supplied calendar values and `Machine::default()` retains its compatibility callback. Shared archive/filename codecs live outside hardware state. Broader host configuration and physical native acceptance remain open. Keep the hardware memory map, device addressing, byte streams, EOI behavior and error responses explicit while separating application responsibilities.

Base/option ROM bytes remain immutable compatibility assets. Do not edit the inherited web assets; verify lengths and stable digests in tests before changing how they are packaged.

== Current worker and GUI flow

1. The GUI polls the latest immutable snapshot without waiting for the worker. It uploads changed display revisions, lays out widgets and submits ordered controls/input through bounded nonblocking command delivery. It does not advance CPU execution during a repaint.
2. The worker exclusively owns mutable machine/runtime/input state. It processes bounded command batches and advances at most one logical 10 ms tick per iteration, coalescing excess elapsed CPU catch-up while retaining wall-time erase callbacks. `HostRuntime` remains the deterministic elapsed-time scheduler.
3. Input retains source ordering, ownership and the existing minimum 100 ms panel press/release intervals. Focus loss and other interruptions release or cancel input through the worker. Readiness changes carry request identifiers so stale snapshots cannot undo lifecycle-driven clears.
4. Before a native file chooser opens, the worker acknowledges a matching file-operation identifier with execution suspended. A separate dialog/import-preparation job uses that boundary; later file/archive/PNG work runs on the worker before modal completion. Export names and writes therefore use the same suspended store.
5. Display color conversion precedes worker publication, not GUI repainting. GUI polling retains the last view while worker-side I/O is busy. Command saturation and worker failures are explicit; shutdown is requested atomically and the worker is joined only after the native event loop returns.

Headless regressions cover GUI interaction with absent worker progress, production x100 execution, and a real host export stalled on a gated FIFO. Physical native window/dialog behavior still needs validation. These execution guarantees must survive any later module reorganization; they do not imply that current GUI-specific types already belong on the correct side of the proposed boundaries. Audio and printer adapters remain outside this implemented flow.

The core should expose snapshot-able state or narrow test interfaces for instruction traces, memory/PIA state, GPIB transcripts, storage records, and display output. The UI should be testable at the action/state level without requiring the CPU implementation to depend on egui.

== Scope and non-goals

The port should preserve the 4051-oriented behavior and the supplied data required by that behavior. The root Cargo workspace contains the UI-independent library and a native eframe application under `dev/src/4051-Emulator/rs/4051-Emulator-rs`; the application presents the display framebuffer, forwards host keyboard events, and advances `HostRuntime`. Its CPU-speed button cycles through the reference's x1, x2, x5, x10, x20, x50, and x100 settings; observed real-time performance and physical-keyboard behavior remain unverified. Ten clickable user keys and five line-editor keys send the reference codes; holding host Shift selects the corresponding alternate functions. Auto Number, Step Program, Home/Page, Rub Out, and Break also send their reference codes through the same input queue. Unlike the web's immediate pointer transitions, the native adapter extends short presses and intervening releases to at least 100 ms, queuing rapid taps to give the emulated keyboard time to observe each transition. The line-editor, program, and special keyboard buttons have been checked at the keyboard PIA, not yet as native editing, interrupt, or program-control workflows. Other front-panel widgets and peripheral panels remain future work. Do not expand to 4052/4054 hardware, rewrite original ROM contents, remove the web reference, or import old duplicate files without evidence that the running HTML depends on them.

= Parity evidence and unresolved decisions

Continue running the web reference and capture a reproducible behavior baseline: boot/reset, representative BASIC and graphics programs, keyboard mappings, screen output, speed settings, GPIB device interactions, storage import/export, and printer/audio behavior. Keep the same program and data assets for Rust comparisons.

Use focused evidence at each layer: CPU reset/interrupt/instruction traces; memory and PIA reads/writes; keyboard-to-Tek-code mappings; logical screen snapshots; GPIB byte transcripts and EOI endings; file metadata/content round trips; and observable UI actions. Compare at the logical display resolution before comparing scaled desktop screenshots. Include malformed input and user-cancel paths so failures remain visible and do not become silent success.

The draft baseline captures startup, control settings, GPIB registry changes, tape import, Auto Load, in-memory reload behavior, and a vector program. The newer browser suites exercise additional supplied programs with documented skips and known reference limitations. Reconcile them before freezing; character-size rendering, physical host-key input, printer output, and broader GPIB device byte/EOI transcripts remain unverified.

The `keyboard_entered_basic_program_matches_reference_display` Rust test enters `10 PRINT 1` and `RUN` through the machine/runtime API and compares a stored-pixel mask with the web reference. Separate machine/runtime tests load the same program through `OLD@1:` or tape using `FIND@5:1` and `OLD@5:`; the tape test imports a Flash Drive filename and bytes through the core API. Another saves, modifies, and reloads the program before running it. These tests check the printed output but do not exercise physical keyboard input, actual native chooser interaction, complete tape storage, or general BASIC compatibility. Native editor workflows and broader option-ROM programs remain unverified.

Native multi-file Open dialogs for Flash Drive import and screen-copy Save As are selected and implemented, but actual native interactions remain unverified. Other storage dialogs and printer presentation still need decisions and implementation. Confirm the deferred baseline environment-mode choice before freezing. Explicitly document intentional user-visible deviations, including native pointer/modal timing and error handling, rather than describing them as complete reference parity.

The companion `graded-migration-guide.typ` turns these findings into staged completion gates. A grade is a migration stage, not a quality score; passing a grade requires its evidence, not merely compiling the current code.
