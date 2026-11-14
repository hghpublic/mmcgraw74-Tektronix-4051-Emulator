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
  Updated: 14 November 2026
]

#outline(title: [Contents], depth: 2)

= Purpose and scope

This design records the web emulator's boundaries, the current Rust/egui implementation, and the remaining target architecture. The web implementation remains the compatibility reference. Isolated comparisons exist, but they do not establish complete parity.

The selected target is a native egui application using eframe. The repository-root `Cargo.toml` is a virtual workspace; its member package manifest, library, and native binary live under `dev/src/4051-Emulator/rs/4051-Emulator-rs`. The package currently depends on eframe 0.32.3.

The user selected storage behavior matching the reference's in-memory tape store. The draft baseline observed saved tape files disappearing on browser reload, despite the README's browser-local-storage claim. Revisit the selected policy only if contrary runtime evidence emerges.

The UI-independent Rust core implements a subset of MC6800 instructions, memory mapping, PIA behavior, interrupts, keyboard signals, display modeling, and host timing. A native eframe shell presents the display and a subset of controls. A bounded startup trace reaches the ROM keyboard routine, and one keyboard-entered BASIC program runs in a machine/runtime test; full boot, complete CPU/board behavior, native input, and feature parity remain unverified.

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

The root project README describes a Motorola 6800, 32 KB RAM, 32 KB BASIC ROM, and 1024 by 780 vector graphics area, together with the supported 4051 option-ROM set (`README.md:109-127`). The machine implementation routes memory reads and writes through RAM, ROM, PIAs, and the GPIB-facing board logic (`TEKTRONIX4051.js:489-803`). The CPU executes batches on a 10 ms interval, while display phosphor decay runs at 100 ms; the speed factor changes the CPU's instructions-per-interval (`TEKTRONIX4051.js:1201-1209, 1310-1318; mc6800.js:60-66, 326-331, 2409-2412`). Keep instruction execution deterministic and independently testable; let the egui frame schedule request bounded batches rather than making an unbounded CPU loop part of `update`.

== Screen and keyboard

The logical display is not a normal text terminal. It plots vectors and character dots from PIA state, retains pixels above a brightness threshold, and erases pixels during refresh (`TekDisplay.js:63-145, 159-241, 307-329`). Preserve coordinate direction, offsets, pixel intensity, and coarse/mixed/fine dot sizing before optimizing the renderer. The current HTML canvas is 1046 by 830, while the user-facing project description specifies the 1024 by 780 graphics space; keep the logical plotting area distinct from the surrounding front-panel UI (`Tektronix4051.html:58; README.md:109-113`). The Rust `DisplayModel` keeps the reference-sized canvas as row-major green-channel intensity values, with PIA reads/writes driving vector, cursor, and character-dot output. `HostRuntime` applies the 100 ms phosphor update and completes the 500 ms erase transition; the native shell uploads framebuffer revisions as an egui texture.

The host-key mapping includes ordinary characters, modifier state, Tektronix function keys, HOME/PAGE, RUB OUT, BREAK, and other special keys. Front-panel keys produce press and release events, and the Auto Load control types a command sequence into the emulated keyboard (`TekKeyboard.js:11-93, 279-365; TEKTRONIX4051.js:1213-1275`). Treat both physical-key mapping and clickable front-panel input as required interaction surfaces.

== GPIB and storage

The default devices are the program loader at address 1, real-time clock at address 2, and 4924 tape drive at address 5 (`Tektronix4051.html:304-318`). The GPIB class handles device registration, address decoding, input/output buffering, and device dispatch (`GPIB.js:3-18, 54-177`). The RTC returns a local date/time string in `DD-MMM-YY HH:MM:SS:SSS` form (`RTC.js:11-39`).

The tape UI exposes file number, type, usage, description, size, selected drive, viewer, save/delete/select operations, and storage clearing (`Tektronix4051.html:204-257`). Import/export has three distinct formats: a single file, a storage archive, and a collection of 4050 Flash Drive files. Archive import/export uses JSZip; Flash Drive filenames encode file metadata; drag/drop is also supported (`Storage.js:200-230, 265-395, 420-541, 1333-1428`). GPIB tape commands include save, old/load, directory, print, input, read, write, binary load/save, list, and find (`Storage.js:51-65, 770-1050`). Keep byte-level GPIB behavior and file-format conversion separate from UI dialogs.

== Documented limitations

The project README explicitly says R12 MUSIC and SOUND commands do not play on the emulator (`README.md:86`). Preserve that as the current compatibility boundary; adding sound synthesis would be a separate feature, not a parity requirement. The UI's beep/click audio toggle is separate from BASIC's R12 SOUND/MUSIC behavior (`TEKTRONIX4051.js:153-157, 914, 1145-1166`).

= Rust/egui architecture

== Current implementation

#table(
  columns: (1.2fr, 3.8fr),
  stroke: 0.5pt,
  [*Component*], [*Implemented responsibility*],
  [#raw("cpu.rs")], [MC6800 registers, instruction subset, memory-bus access, and interrupt entry/return.],
  [#raw("machine.rs")], [Owns CPU and bus; samples interrupt sources and exposes bounded stepping, keyboard, PIA, and display operations.],
  [#raw("machine_bus.rs"), #raw("pia.rs"), #raw("rom.rs")], [Map RAM, base and selected option ROM payloads, PIA registers, keyboard and display signals, and board interrupt flags.],
  [#raw("display.rs")], [Models the logical framebuffer, PIA-driven plotting, granularity, phosphor refresh, and erase transitions.],
  [#raw("host_runtime.rs")], [Advances caller-supplied elapsed time through CPU and keyboard callbacks, display refreshes, and erase completion.],
  [#raw("main.rs"), #raw("keyboard_input.rs"), #raw("front_panel.rs")], [Run the eframe shell, present framebuffer revisions and settings, translate host events, and schedule front-panel pointer input.],
)

The machine maps base and overflow ROMs, the backpack payloads, and all eight slots of each ROM expander. Selector 2/3 reads within the switchable window deliberately return zero for the reference's undefined bytes; unsupported selector-1 and selector-4 tails return errors. Board interrupt routing uses latched PIA flags as in the web reference. These choices do not establish full undefined-value or physical hardware parity.

The host runs CPU and keyboard callbacks at 10 ms, phosphor refresh at 100 ms, and erase completion after 500 ms. Each CPU callback has an instruction budget derived from the selected speed. Machine errors stop the runtime and are shown in the UI; invalid speed settings are reported without replacing the prior setting. The app does not currently expose recovery controls for a stopped runtime.

Native GPIB routing and devices, tape storage/import, Auto Load, printer, audio, screen copy, and remaining lifecycle/front-panel controls are future work. The machine supports Caps Lock state, but the native adapter does not wire a Caps Lock control. The current window has been launched and an earlier shell was visually observed; native input and presentation parity remain unverified.

== Target separation and ownership

- *Emulator core:* Rust state and behavior for MC6800 registers/instructions, memory, ROM mapping, PIA registers, interrupt signals, keyboard state, and eventually GPIB protocol state. No egui types, filesystem access, dialogs, or global mutable state.
- *Machine orchestration:* `Machine` owns CPU and board components and executes bounded instruction batches; `HostRuntime` schedules batches at the selected speed. The hardware memory map and ordering remain explicit.
- *Display model:* the machine bus translates display PIA accesses into vector, cursor, and character-dot operations on `DisplayModel`. It owns phosphor persistence/decay semantics and exports an immutable green-channel framebuffer suitable for egui painting or texture upload.
- *Peripheral layer, planned:* a typed GPIB device interface with Loader, RTC, tape storage, and Printer implementations. Device addressing, byte streams, EOI behavior, and error responses are core contracts.
- *Storage model and codecs, planned:* numbered file records, header fields, binary content, Flash Drive filename parsing, and archive encoding/decoding live outside UI code. UI file pickers and filesystem paths are adapters.
- *egui application:* owns the implemented presentation, settings, and input adapters; dialogs, audio preferences, and lifecycle controls remain planned. It sends operations to the machine and presents resulting state rather than directly editing CPU or PIA registers.
- *Immutable compatibility assets:* port the existing base/option ROM bytes as data without editing the inherited web assets. Verify lengths and stable digests in tests before changing how they are packaged.

== Frame and event flow

1. Advance elapsed time using the previous frame's modifiers and speed. `FrontPanel` divides the interval at queued pointer transitions and calls `HostRuntime`, which executes due machine and display callbacks.
2. Upload the read-only framebuffer when its revision changes, then lay out the display, controls, and panel. A speed selection affects subsequent elapsed time, not the interval already advanced.
3. Process host-key and pointer events in source order. The keyboard adapter translates supported keys and modifiers into machine operations; the panel queues rapid pointer taps and enforces minimum 100 ms press and release intervals.
4. On focus loss, clear held input and pending panel presses. Request the next repaint while the runtime is healthy; display an execution error if it stops.

File, audio, and printer requests are planned application adapters, not part of this current frame flow. Real-time responsiveness under sustained or delayed frames still needs native validation.

The core should expose snapshot-able state or narrow test interfaces for instruction traces, memory/PIA state, GPIB transcripts, storage records, and display output. The UI should be testable at the action/state level without requiring the CPU implementation to depend on egui.

== Scope and non-goals

The port should preserve the 4051-oriented behavior and the supplied data required by that behavior. The root Cargo workspace contains the UI-independent library and a native eframe application under `dev/src/4051-Emulator/rs/4051-Emulator-rs`; the application presents the display framebuffer, forwards host keyboard events, and advances `HostRuntime`. Its CPU-speed button cycles through the reference's x1, x2, x5, x10, x20, x50, and x100 settings; observed real-time performance and physical-keyboard behavior remain unverified. Ten clickable user keys and five line-editor keys send the reference codes; holding host Shift selects the corresponding alternate functions. Auto Number, Step Program, Home/Page, Rub Out, and Break also send their reference codes through the same input queue. Unlike the web's immediate pointer transitions, the native adapter extends short presses and intervening releases to at least 100 ms, queuing rapid taps to give the emulated keyboard time to observe each transition. The line-editor, program, and special keyboard buttons have been checked at the keyboard PIA, not yet as native editing, interrupt, or program-control workflows. Other front-panel widgets and peripheral panels remain future work. Do not expand to 4052/4054 hardware, rewrite original ROM contents, remove the web reference, or import old duplicate files without evidence that the running HTML depends on them.

= Parity evidence and unresolved decisions

Continue running the web reference and capture a reproducible behavior baseline: boot/reset, representative BASIC and graphics programs, keyboard mappings, screen output, speed settings, GPIB device interactions, storage import/export, and printer/audio behavior. Keep the same program and data assets for Rust comparisons.

Use focused evidence at each layer: CPU reset/interrupt/instruction traces; memory and PIA reads/writes; keyboard-to-Tek-code mappings; logical screen snapshots; GPIB byte transcripts and EOI endings; file metadata/content round trips; and observable UI actions. Compare at the logical display resolution before comparing scaled desktop screenshots. Include malformed input and user-cancel paths so failures remain visible and do not become silent success.

The draft baseline captures startup, control settings, GPIB registry changes, tape import, Auto Load, in-memory reload behavior, and a vector program. The newer browser suites exercise additional supplied programs with documented skips and known reference limitations. Reconcile them before freezing; character-size rendering, physical host-key input, printer output, and GPIB byte/EOI transcripts remain unverified.

The `keyboard_entered_basic_program_matches_reference_display` Rust test enters `10 PRINT 1` and `RUN` through the machine/runtime API and compares a stored-pixel mask with the web reference. It does not exercise physical keyboard input, exact pixel intensities, storage loading, or general BASIC compatibility. Native editor workflows and option-ROM programs also remain unverified.

The selected native target still needs decisions and implementations for file dialogs and printer presentation. Confirm the deferred baseline environment-mode choice before freezing. Explicitly document any intentional user-visible deviation, including the native pointer timing and error handling, rather than describing it as reference parity.

The companion `graded-migration-guide.typ` turns these findings into staged completion gates. A grade is a migration stage, not a quality score; passing a grade requires its evidence, not merely compiling the current code.
