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
  #text(size: 15pt)[Rust/egui Migration Plan]
]

#align(center)[
  Web reference: `dev/src/4051-Emulator/web/4051-Emulator`
  \
  Workspace manifest: repository-root `Cargo.toml`
  \
  Rust implementation root: `dev/src/4051-Emulator/rs/4051-Emulator-rs`
  \
  Plan basis: reviewed source, `design.typ`, and `graded-migration-guide.typ`
  \
  Date: 12 November 2026
]

#outline(title: [Contents], depth: 2)

= Purpose and current state

This plan sequences the port of the reviewed Tektronix 4051 web emulator to Rust with egui. Its goal is observable feature parity with the existing 4051-oriented application and supplied data, not a broader 4050-series emulator or a replacement of the web reference.

The root manifest is a virtual workspace with resolver 3. The member manifest at `dev/src/4051-Emulator/rs/4051-Emulator-rs/Cargo.toml` defines the package and `[lib] path = "lib.rs"`. Integration tests are automatically discovered under the member's `tests/` directory. Root and member commands share the repository-root lockfile and build-output directory.

The Cargo manifest is the repository-root `Cargo.toml`; Rust implementation files belong under `dev/src/4051-Emulator/rs/4051-Emulator-rs`. The native target is confirmed. The UI-independent Rust core now includes selected 8/16-bit operations and addressing modes, base/system-overflow ROM mapping, selector-4/5 backpack payloads, all eight selector-6 left expander slots, explicit ignored banks, RAM, PIA registers, keyboard press/release and modifier state, reference-compatible board IRQ/NMI routing, display erase/ready signaling, interrupt entry/return, and bounded instruction stepping; 55 tests pass. Without the host's initial `display_ready()` call, a bounded 100,000-instruction run waits at `0xC6B4` with interrupts masked. Supplying readiness releases that wait without a CPU register override; startup executes selector-1 overflow code, probes ignored banks 2/3, reads both backpack headers and the first left-expander slot header, then reaches an explicit unsupported selector-7 read at `0x8800` with the CPU interrupt mask clear. The web `DERROML` array contains 2 KiB although its selector covers an 8 KiB window; Rust maps its exact bytes at `0x8800..0x9000` and errors on the uncovered tail rather than inventing bytes. The web `DERROMR` array contains a complete 8 KiB of `0xFF`; Rust maps those bytes throughout `0x8800..0xA7FF`. Selector 6 maps each of the eight 8 KiB `BSXLSLOT1`–`BSXLSLOT8` payloads into `0x8800..0xA7FF` according to the low three address-field bits; the concatenated 64 KiB image is source-derived and fingerprinted. The accepted ignored-bank policy maps only selectors 2/3 to zero bytes in the bank window, matching the reference word-probe result while deliberately normalizing JavaScript undefined byte values to Rust `u8`; other unsupported mappings remain errors. The separate keypress/release handler trace still explicitly clears the CPU interrupt mask; normal startup now clears the mask by selector 7, but timed refresh and an end-to-end keyboard IRQ trace remain unverified. Broader CPU coverage, host-timed keyboard refresh/erase completion, selector-7/right-expander program coverage, peripherals, and application-level parity remain incomplete.

The plan's gates align with `graded-migration-guide.typ`. The user selected native desktop and matching the reference's apparent in-memory tape behavior. No calendar durations are assigned; a vector-program baseline is now captured, but peripheral evidence remains incomplete.

= Delivery approach

== Principles

- Treat the web implementation and its supplied ROM/program data as the behavior oracle. Do not modify inherited web files or assets.
- Separate the deterministic emulator core from egui, host file access, and presentation.
- Preserve the current implementation's documented limitations; R12 MUSIC/SOUND support is not part of parity.
- Require observable evidence at each gate. A compiling UI or an implemented type alone does not demonstrate feature parity.
- Surface device, file, and import/export errors; do not convert failures into empty data or success-shaped state.

== Dependency sequence

#table(
  columns: (0.75fr, 2.2fr, 2.2fr),
  stroke: 0.5pt,
  [*Phase*], [*Depends on*], [*Primary completion evidence*],
  [0], [None], [Executable web baseline, acceptance checklist, platform and persistence decisions],
  [1], [Phase 0], [Cargo package at the defined paths; deterministic core tests and CPU/reference traces],
  [2], [Phase 1], [Interactive egui screen, key mapping, display and lifecycle tests],
  [3], [Phase 1 core contracts], [GPIB/device transcripts and storage/import-export tests],
  [4], [Phases 2 and 3], [Option-ROM program runs and remaining peripheral/controls checks],
  [5], [Phases 0-4], [Complete parity evidence matrix, successful format/lint/build/test checks],
)

Phases 2 and 3 may proceed as separate work streams after the machine/GPIB interface is stable. Their UI integration and final acceptance remain gated by the phase exits below.

= Phase 0 — Baseline and decisions

*Objective:* make the parity target executable and unambiguous before translating code.

*Work items:*

- P0.1 Record the exact web source revision and launch procedure. Verify startup, reset, keyboard input, and a known BASIC/vector program.
- P0.2 Exercise the documented user flows: keyboard and panel keys, pixel modes, scaling, speed selection, program loading, Auto Load, GPIB devices, storage operations, printer, and UI audio.
- P0.3 Create a compact baseline corpus: program inputs, keyboard sequences, logical display captures, CPU/memory checkpoints where observable, GPIB byte/EOI transcripts, and storage metadata/content expectations. Keep the existing source assets unchanged.
- P0.4 Record the user's decision to match the reference's apparent in-memory tape behavior. The source README mentions browser-local storage, while the inspected `Storage.js` creates an in-memory `fileStore`; flag any contrary runtime evidence.
- P0.5 Use the confirmed native desktop target. This determines eframe configuration and host file, persistence, download, and printer adapters.
- P0.6 Freeze the feature boundary, including the documented unsupported R12 SOUND/MUSIC behavior and the set of example programs used for parity.

*Dependencies:* none.

*Exit gate:* reference application launch is repeatable; acceptance checklist and corpus are recorded; deployment target and persistence policy are decided. The reference now has a pinned source revision, a reproducible local launch, and captured `prog.txt` vector-program runs at all three display-granularity settings, control cycles, GPIB registry changes, and in-memory tape behavior. Character-size rendering, printer output, physical keyboard behavior, and GPIB byte/EOI transcripts remain open, so Phase 0 is not complete. If the web reference fails to launch or a behavior cannot be observed, mark it blocked and resolve the reference state before claiming parity.

= Phase 1 — Cargo package and deterministic machine core

*Objective:* establish the Rust build and a testable machine foundation without coupling the emulator to egui.

*Work items:*

- P1.1 Configure repository-root `Cargo.toml` as a virtual workspace with its member package manifest under `dev/src/4051-Emulator/rs/4051-Emulator-rs`. Keep one package with a testable core library and a future application entry point; add further member crates only when justified. Preserve root-level Cargo commands and automatic member test discovery.
- P1.2 Add the minimum dependencies needed for the selected target. Keep UI, file dialogs, and platform-specific features out of the core library.
- P1.3 Implement MC6800 registers, instructions, flags, addressing modes, reset and interrupt behavior, and fixed-width memory operations.
- P1.4 Implement the 4051 board memory map, 32 KB RAM, base ROM mapping, PIA registers/signals, keyboard input state, display erase/ready handshakes, and machine interrupt/control behavior. The host signals initial display readiness and acknowledges erase completion; the deterministic core must not guess host elapsed time. Ignored selectors 2/3 now explicitly read zero only in `0x8800..0xA7FF`; genuinely unsupported mappings remain errors.
- P1.5 Integrate the existing base ROM data, selector-1 system overflow ROMs U101/U201 (`ROM747`/`ROM748`), selector-4/5 left/right backpack images (`DERROML`/`DERROMR`), and selector-6 left ROM-expander slots as immutable compatibility input. Check expected lengths and stable digests; do not edit the web source ROM. The 2 KiB left-backpack image maps read-only at `0x8800..0x8FFF` with its uncovered selector window explicit as errors. The right-backpack image is the exact 8 KiB reference payload, currently all `0xFF`, and maps read-only over `0x8800..0xA7FF`. Selector 6 maps the eight exact 8 KiB `BSXLSLOT1`–`BSXLSLOT8` images read-only across that window, selected by the bank register's low three address bits. Startup probes both backpack headers and the first expander-slot header, then reaches selector 7. Full right-expander and option-ROM program coverage remains Phase 4.
- P1.6 Add focused CPU and board tests, then compare deterministic reset, instruction, memory, PIA, and interrupt traces against Phase 0 observations.
- P1.7 Expose a bounded, deterministic machine-step API that can later be called from the egui frame loop.

*Dependencies:* Phase 0 decisions.

*Exit gate:* the package builds and tests from the repository root; the machine reaches the reference reset state; selected CPU/board traces match; the base ROM executes in the covered cases without panic or unexplained state divergence.

= Phase 2 — egui shell, display, and keyboard

*Objective:* provide the main interactive front panel while preserving the original screen and input behavior.

*Work items:*

- P2.1 Create the application shell under `dev/src/4051-Emulator/rs/4051-Emulator-rs`; connect egui lifecycle actions to the machine API without embedding hardware behavior in UI callbacks.
- P2.2 Implement host-key translation, modifier state, Tektronix function keys, press/release semantics, and clickable front-panel controls.
- P2.3 Implement the logical vector/character display, cursor, coordinate transforms, pixel modes, erase behavior, phosphor persistence/decay, and screen copy. Call `display_ready()` at execution start; when `display_erase_pending()` is true, present the erase and complete it after the reference's 500 ms host delay before calling `display_ready()` again.
- P2.4 Reproduce start, pause, reset, status indicators, display scaling, speed controls, and UI beep/click mute behavior.
- P2.5 Schedule bounded CPU work from elapsed time so host frame rate does not determine emulated speed. Calibrate using the reference workload and speed settings.
- P2.6 Add mapping tests for keyboard events, framebuffer/pixel tests at logical resolution, and egui action/state tests for controls and resize behavior.

*Dependencies:* Phase 1 machine API and Phase 0 visual/input baseline.

*Exit gate:* the Rust application boots, accepts representative typed and panel input, draws a representative character/vector program, and passes focused input, display, lifecycle, scaling, and speed checks.

= Phase 3 — GPIB, devices, and tape storage

*Objective:* run programs through the emulated loader and 4924 tape workflows.

*Work items:*

- P3.1 Implement the GPIB addressing, talk/listen transitions, byte buffers, EOI handling, device registry, and error behavior as core contracts.
- P3.2 Implement the program Loader and RTC at their reference default addresses and preserve the RTC string format.
- P3.3 Implement the numbered tape-file model, metadata, multiple drives, and BASIC save/load/directory/find/read/write/list/binary/print commands.
- P3.4 Implement single-file import/export, storage-archive ZIP, 4050 Flash Drive file sets, filename metadata parsing, and supported drag/drop/file-picker workflows in application adapters.
- P3.5 Add egui storage and GPIB management panels, including selection, save/delete/clear, device add/remove, overwrite confirmation, and visible success/error results.
- P3.6 Test GPIB transcripts, EOI endings, storage command behavior, malformed files/archives, cancellation, and byte-for-byte import/export round trips.
- P3.7 Run the documented `FIND@5`, `OLD@5`, `RUN` flow using a supplied program and test multiple storage devices.

*Dependencies:* Phase 1 machine/GPIB contracts; Phase 0 file-format and persistence decisions. Core GPIB/storage work can proceed alongside Phase 2 presentation work if the machine interface is stable.

*Exit gate:* Loader, RTC, and tape operations work through BASIC; expected metadata/content and GPIB transcripts match the reference; supported file formats round-trip; errors and cancellations are visible and do not leave success-shaped state.

= Phase 4 — Extended ROMs and remaining behavior

*Objective:* bring the supported option-ROM programs and remaining user-facing peripherals to parity.

*Work items:*

- P4.1 Integrate option cartridge/expander ROM bytes and reproduce bank/slot selection. Verify all transferred bytes against the original assets. Required selector-1 system overflow ROM integration belongs in Phase 1 because normal startup accesses it.
- P4.2 Run representative examples for R01, R05, R06, R07, R08, R12, R14, DDT51, Extended BASIC, and Extended Fonts.
- P4.3 Implement GPIB Printer output using the agreed native egui presentation.
- P4.4 Verify all supported CPU speed factors, audio mute/beep/click behavior, copy-screen output, scaling, and status indicators.
- P4.5 Recheck compatibility with the chosen persistence policy and the supplied program corpus. Do not add BASIC R12 SOUND/MUSIC as a parity feature unless separately requested.

*Dependencies:* Phases 2 and 3, plus Phase 0 target decision.

*Exit gate:* representative programs exercise every documented option-ROM family; all remaining documented UI/peripheral functions pass their checks; known unsupported sound behavior remains accurately stated.

= Phase 5 — Parity audit and delivery

*Objective:* demonstrate complete agreed coverage and prepare a maintainable Rust package.

*Work items:*

- P5.1 Replay the Phase 0 checklist from a clean application state on every supported target.
- P5.2 Compare CPU/memory traces, logical display output, keyboard mappings, GPIB transcripts, storage state and round trips, printer output, and visible errors against the reference corpus.
- P5.3 Exercise boundary cases: empty/invalid program input, missing tape files, invalid device addresses, malformed archive, canceled import/export, delete-all cancellation, reset while running, and reload persistence if required.
- P5.4 Run Rust formatting, lint, build, and focused/full tests using the configured repository-root manifest.
- P5.5 Document intentional divergences and remaining unsupported behavior. Keep the web emulator and supplied data as the reference; cleanup/removal is not part of this migration.

*Dependencies:* Phases 0-4.

*Exit gate:* every in-scope behavior has recorded evidence; all required checks pass; remaining gaps are explicit; no unresolved platform or persistence decision affects the claimed parity.

= Feature-to-phase coverage

#table(
  columns: (2fr, 0.9fr, 2.1fr),
  stroke: 0.5pt,
  [*Feature group*], [*Phase*], [*Sign-off evidence*],
  [MC6800, board map, RAM, base ROM], [1], [CPU and memory/PIA differential traces],
  [egui screen, vectors, keyboard, front panel, controls], [2], [Input maps, logical framebuffer checks, lifecycle and scaling checks],
  [GPIB, Loader, RTC, tape commands, import/export], [3], [Bus transcripts, command tests, file-format round trips],
  [Option ROMs, Printer, remaining audio/speed behavior], [4], [Representative programs and peripheral checks],
  [End-to-end parity and delivery checks], [5], [Completed feature matrix and passing Rust validation],
)

= Decisions to close before implementation

- *Target platform:* native desktop using egui; eframe and file-dialog dependencies remain to be selected for the UI phase.
- *Storage reload persistence:* the user selected the inspected in-memory reference behavior; revisit only if runtime evidence contradicts it.
- *Cargo target layout:* use the established root manifest and implementation directory; choose the smallest package structure compatible with the target.
- *Reference baseline:* record a runnable source revision and representative program/data corpus before using outcomes as a hard parity gate.

The previous `npm test` command is a placeholder rather than a useful regression suite (`package.json:6-11`). Do not use it as evidence that the source behavior is validated. The Rust test plan should provide the missing deterministic checks at the machine, display, GPIB, storage, and application layers.
