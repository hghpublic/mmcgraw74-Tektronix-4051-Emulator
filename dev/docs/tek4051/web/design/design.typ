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
  Cargo manifest: repository-root `Cargo.toml`
  \
  Status: native target selected; expanded CPU instruction subset implemented; parity baseline incomplete
  \
  Date: 12 November 2026
]

#outline(title: [Contents], depth: 2)

= Purpose and scope

This design records the current web emulator's boundaries and proposes a Rust/egui architecture that can preserve its observable behavior. The web implementation remains the compatibility reference. This work does not change it, remove it, or claim runtime parity has already been measured.

The requested target is a native egui application. The user selected native desktop; eframe is the intended application integration but has not yet been added. Rust implementation files belong under `dev/src/4051-Emulator/rs/4051-Emulator-rs`, with the package manifest at repository-root `Cargo.toml`.

The user selected storage behavior matching the reference's apparent in-memory tape store. This resolves the port's initial policy, while the README's browser-local-storage claim remains inconsistent with the inspected `Storage.js`; revisit only if runtime evidence contradicts the selected behavior.

The Rust package now contains a UI-independent CPU and machine core with selected immediate/direct/indexed/extended instructions, word operations, unary operations, branches, subroutines, and interrupt entry/return. It maps 32 KB RAM, the base ROM, and PIA registers, and executes the first twelve base-ROM reset instructions. It is not yet a full MC6800 or board implementation, and it does not boot the complete system or provide the egui application.

The compatibility target is the behavior actually established by the reference implementation and its supplied examples, including its documented limitations. It is not a claim to emulate every Tektronix 4051 peripheral or every feature of later 4050-series models.

= Reviewed system

== Entry point and runtime composition

`Tektronix4051.html` is the application entry point. It loads the CPU, base ROM, display, keyboard, machine, storage, ZIP, ROM expansion, RTC, loader, GPIB, and printer scripts directly, then constructs default GPIB devices at addresses 1, 2, and 5. The page owns the canvas, keyboard controls, status indicators, speed and pixel-mode buttons, storage dialog, and GPIB dialog (`Tektronix4051.html:8-22, 37-58, 186-192, 300-318`).

The page's JavaScript also connects start, pause, reset, auto-scaling, CPU speed, display granularity, file loading, audio, and device-registration actions to the emulator (`Tektronix4051.html:364-404, 465-546, 605-621`). The stylesheet contains the fixed front-panel layout and screen presentation (`styleSheet.css:1-160`).

== Logical responsibilities in the current implementation

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

The source directory has a `package.json` with ESLint development dependencies and a placeholder `npm test` command that exits with “Error: no test specified” (`package.json:1-16`). The HTML loads a classic-script `RTC.js`; a separate `RTC.mjs` is present but is not in that script list. Likewise, `.old` and `.new` files in the folder are not loaded by the application entry point. Establish the canonical source from the HTML dependency list; do not automatically port duplicate or unused files.

`Storage.js` uses a `fileStore` array initialized with each `Storage` object (`Storage.js:38-40, 1076-1210`). The source README says browser-local storage was added (`README.md:22`), but the reviewed storage implementation contains no `localStorage`, `sessionStorage`, or IndexedDB calls. Whether data survives a browser reload therefore needs a baseline runtime check before persistence is classified as a parity requirement.

= Behavior and compatibility surface

== Machine execution

The root project README describes a Motorola 6800, 32 KB RAM, 32 KB BASIC ROM, and 1024 by 780 vector graphics area, together with the supported 4051 option-ROM set (`README.md:109-127`). The machine implementation routes memory reads and writes through RAM, ROM, PIAs, and the GPIB-facing board logic (`TEKTRONIX4051.js:489-803`). The CPU executes batches on a 10 ms interval, while display phosphor decay runs at 100 ms; the speed factor changes the CPU's instructions-per-interval (`TEKTRONIX4051.js:1201-1209, 1310-1318; mc6800.js:60-66, 326-331, 2409-2412`). Keep instruction execution deterministic and independently testable; let the egui frame schedule request bounded batches rather than making an unbounded CPU loop part of `update`.

== Screen and keyboard

The logical display is not a normal text terminal. It plots vectors and character dots from PIA state, retains pixels above a brightness threshold, and erases pixels during refresh (`TekDisplay.js:63-145, 159-241, 307-329`). Preserve coordinate direction, offsets, pixel intensity, and coarse/mixed/fine dot sizing before optimizing the renderer. The current HTML canvas is 1046 by 830, while the user-facing project description specifies the 1024 by 780 graphics space; keep the logical plotting area distinct from the surrounding front-panel UI (`Tektronix4051.html:58; README.md:109-113`).

The host-key mapping includes ordinary characters, modifier state, Tektronix function keys, HOME/PAGE, RUB OUT, BREAK, and other special keys. Front-panel keys produce press and release events, and the Auto Load control types a command sequence into the emulated keyboard (`TekKeyboard.js:11-93, 279-365; TEKTRONIX4051.js:1213-1275`). Treat both physical-key mapping and clickable front-panel input as required interaction surfaces.

== GPIB and storage

The default devices are the program loader at address 1, real-time clock at address 2, and 4924 tape drive at address 5 (`Tektronix4051.html:304-318`). The GPIB class handles device registration, address decoding, input/output buffering, and device dispatch (`GPIB.js:3-18, 54-177`). The RTC returns a local date/time string in `DD-MMM-YY HH:MM:SS:SSS` form (`RTC.js:11-39`).

The tape UI exposes file number, type, usage, description, size, selected drive, viewer, save/delete/select operations, and storage clearing (`Tektronix4051.html:204-257`). Import/export has three distinct formats: a single file, a storage archive, and a collection of 4050 Flash Drive files. Archive import/export uses JSZip; Flash Drive filenames encode file metadata; drag/drop is also supported (`Storage.js:200-230, 265-395, 420-541, 1333-1428`). GPIB tape commands include save, old/load, directory, print, input, read, write, binary load/save, list, and find (`Storage.js:51-65, 770-1050`). Keep byte-level GPIB behavior and file-format conversion separate from UI dialogs.

== Documented limitations

The project README explicitly says R12 MUSIC and SOUND commands do not play on the emulator (`README.md:86`). Preserve that as the current compatibility boundary; adding sound synthesis would be a separate feature, not a parity requirement. The UI's beep/click audio toggle is separate from BASIC's R12 SOUND/MUSIC behavior (`TEKTRONIX4051.js:153-157, 914, 1145-1166`).

= Proposed Rust/egui architecture

== Separation and ownership

- *Emulator core:* pure Rust state and behavior for MC6800 registers/instructions, memory, ROM mapping, PIA registers, interrupt signals, keyboard matrix state, and GPIB electrical/protocol state. No egui types, filesystem access, dialogs, or global mutable state.
- *Machine orchestration:* owns CPU and board components, advances bounded work at the selected speed, and exposes input operations plus observable output/events. The hardware memory map and ordering remain explicit.
- *Display model:* consumes machine display signals into a logical framebuffer or ordered drawing-event buffer. It owns phosphor persistence/decay semantics and exports an immutable view suitable for egui painting or texture upload.
- *Peripheral layer:* a typed GPIB device interface with Loader, RTC, tape storage, and Printer implementations. Device addressing, byte streams, EOI behavior, and error responses are core contracts.
- *Storage model and codecs:* numbered file records, header fields, binary content, Flash Drive filename parsing, and archive encoding/decoding live outside UI code. UI file pickers and filesystem paths are adapters.
- *egui application:* owns panels, controls, dialogs, key event translation, user notifications, screen scaling, audio preferences, and the app lifecycle. It sends explicit actions to the machine and presents resulting state; it does not directly edit CPU or PIA registers.
- *Immutable compatibility assets:* port the existing base/option ROM bytes as data without editing the inherited web assets. Verify lengths and stable digests in tests before changing how they are packaged.

== Frame and event flow

1. Translate egui key events and front-panel button presses/releases into a small typed Tektronix input enum.
2. Advance the machine by a bounded amount of emulated time, using an accumulator so host frame rate does not define CPU speed.
3. Let machine I/O produce display updates, GPIB transactions, status changes, and peripheral effects through explicit interfaces.
4. Update the logical display model; paint the screen and front panel through egui.
5. Dispatch file/audio/print requests through application adapters and report failures visibly.

The core should expose snapshot-able state or narrow test interfaces for instruction traces, memory/PIA state, GPIB transcripts, storage records, and display output. The UI should be testable at the action/state level without requiring the CPU implementation to depend on egui.

== Scope and non-goals

The port should preserve the 4051-oriented behavior and the supplied data required by that behavior. The root Cargo package uses a UI-independent library under `dev/src/4051-Emulator/rs/4051-Emulator-rs`; add the native eframe application and platform dependencies when the UI phase begins. Do not expand to 4052/4054 hardware, rewrite original ROM contents, remove the web reference, or import old duplicate files without evidence that the running HTML depends on them.

= Parity evidence and unresolved decisions

Continue running the web reference and capture a reproducible behavior baseline: boot/reset, representative BASIC and graphics programs, keyboard mappings, screen output, speed settings, GPIB device interactions, storage import/export, and printer/audio behavior. Keep the same program and data assets for Rust comparisons.

Use focused evidence at each layer: CPU reset/interrupt/instruction traces; memory and PIA reads/writes; keyboard-to-Tek-code mappings; logical screen snapshots; GPIB byte transcripts and EOI endings; file metadata/content round trips; and observable UI actions. Compare at the logical display resolution before comparing scaled desktop screenshots. Include malformed input and user-cancel paths so failures remain visible and do not become silent success.

The baseline should continue to verify:

- Representative program and peripheral behavior, since only page load, start, and reset have been exercised so far.
- Whether the running web reference contradicts the selected in-memory storage policy. The selected native desktop target uses different file-dialog, persistence, download, and printer adapters from a browser build.

The companion `graded-migration-guide.typ` turns these findings into staged completion gates. A grade is a migration stage, not a quality score; passing a grade requires its evidence, not merely compiling the current code.
