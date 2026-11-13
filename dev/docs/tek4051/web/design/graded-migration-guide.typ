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
  #text(size: 15pt)[Graded Rust/egui Migration Guide]
]

#align(center)[
  Source: `dev/src/4051-Emulator/web/4051-Emulator`
  \
  Implementation root: `dev/src/4051-Emulator/rs/4051-Emulator-rs`
  \
  Cargo manifest: repository-root `Cargo.toml`
  \
  Target: native desktop Rust application using egui
  \
  Date: 12 November 2026
]

#outline(title: [Contents], depth: 2)

= How to use the grades

Progress through the grades in order. Each grade is a completion gate with observable exit evidence, not a percentage or an invented quality score. Keep the JavaScript emulator and its supplied ROM/program assets as the behavior oracle. Do not change inherited source files as part of this migration unless separately authorized.

The web entry point wires the visible controls to global JavaScript modules; the Rust implementation should first reproduce those contracts, then replace browser-specific interactions with egui/native adapters (`Tektronix4051.html:8-22, 37-58, 186-257, 300-318`). Native desktop is selected. The user also selected matching the reference's apparent in-memory tape behavior; the README/runtime discrepancy remains subject to baseline evidence. The design document at `design.typ` describes the target boundaries.

= Grade 0 — Baseline and scope ready

*Goal:* establish which web behaviors actually run and define the parity contract before implementation.

*Work:*

- Select and record the precise web source revision and launch procedure. Verify the page starts, can reset, accepts keyboard input, and renders at least one known program.
- Inventory the exact script dependency list in `Tektronix4051.html`. Exclude unused `.old`, `.new`, alternate module, and test files unless runtime evidence shows they are required.
- Build a behavior checklist from the documented 4051 hardware, option ROMs, GPIB tape features, scaling, pixel modes, CPU speed, and keyboard instructions (`README.md:37-58, 109-141`).
- The user selected matching the reference's apparent in-memory storage behavior. The source README mentions browser-local storage (`web/4051-Emulator/README.md:22`), while `Storage.js` initializes an in-memory `fileStore` (`Storage.js:38-40, 1076-1210`); flag contrary runtime evidence without changing the selected policy silently.
- The target is native desktop. Keep file dialogs, downloads, persistence, and printer presentation in native adapters.
- Preserve the source tree and asset files. Record representative program inputs and expected CPU, display, GPIB, and storage outcomes.

*Exit gate:* an executable reference baseline, an agreed feature checklist, target-platform decision, and explicit answers for persistence and known unsupported behavior. If the web reference cannot run reliably, mark affected behaviors “baseline blocked” rather than guessing expected output.

= Grade 1 — Machine core boots deterministically

*Goal:* create a UI-independent Rust machine core that can reset and execute the existing base system.

*Work:*

- Implement MC6800 state, instruction execution, flags, addressing, reset/interrupt vectors, and memory access; use fixed-width integers and checked boundaries.
- The repository-root `Cargo.toml` defines the package and points its core library at `dev/src/4051-Emulator/rs/4051-Emulator-rs/lib.rs`. The current core executes the first twelve base-ROM reset instructions; continue CPU/addressing coverage until the complete startup path is reached.
- The Rust machine now maps 32 KB RAM, the base ROM, PIA data/direction/control registers, and an instruction-budget API. Finish PIA interrupt and keyboard signaling, display-control behavior, GPIB handshake, and expansion-ROM bank mapping (`TEKTRONIX4051.js:489-803, 1126-1209`).
- Preserve the base ROM bytes and test byte count and digest against the reviewed source (`Tek4051Rom.js:1-3, 5960-5982`). Do not transcribe or “clean up” ROM contents.
- Use a deterministic machine-step API that accepts input and returns observable effects; avoid browser globals, wall-clock scheduling, and egui dependencies in this layer.
- Establish instruction-level unit tests and differential traces against the JavaScript reference for reset, representative opcodes/addressing modes, interrupts, and memory-mapped register access.

*Exit gate:* the Rust machine resets to the reference reset vector, matches selected deterministic CPU/memory traces, and executes the base ROM without a panic or state divergence in the covered cases. Do not start the full UI until the core contract is testable.

= Grade 2 — Interactive display and front panel

*Goal:* make the machine usable through an egui screen and keyboard/front-panel controls.

*Work:*

- Reproduce Tektronix key translation, modifiers, key press/release behavior, function keys, HOME/PAGE, RUB OUT, BREAK, and the clickable front-panel keys (`TekKeyboard.js:11-93, 279-365; Tektronix4051.html:72-192`).
- Render the logical vector display and character dots with matching coordinates, colors, dot sizing, cursor behavior, erase timing, persistence, and refresh decay (`TekDisplay.js:63-145, 159-241, 272-329, 342-365`).
- Keep logical plotting coordinates separate from egui scaling and window layout. Match the reference coarse, mixed, and fine display modes and Auto-Scaling behavior (`Tektronix4051.html:40-50, 465-498, 524-547`).
- Implement start, pause, reset, status indicators, screen copy, and UI beep/click mute behavior. Schedule bounded CPU work from elapsed time; preserve the reference's nominal interval and speed-factor behavior (`TEKTRONIX4051.js:1201-1209, 1277-1318; mc6800.js:60-66, 326-331`).
- Compare logical framebuffer output first, then verify scaled-window presentation at varied aspect ratios.

*Exit gate:* a user can boot, type, use panel keys, interrupt/reset, and observe a representative BASIC character-and-vector workload. Key maps, pixel modes, logical screen snapshots, screen copy, and start/pause/reset controls have focused checks.

= Grade 3 — GPIB and mass-storage workflows

*Goal:* make BASIC programs load, communicate with peripherals, and use tape storage end to end.

*Work:*

- Implement an explicit GPIB bus/device interface with address validation, device registration/removal, talk/listen routing, byte buffers, and EOI behavior (`GPIB.js:54-177, 194-287; TEKTRONIX4051.js:288-488`).
- Add the program Loader, RTC, and 4924 tape drive at the default addresses. Preserve the RTC's formatted local time and the machine's GPIB transaction ordering (`Tektronix4051.html:304-318; Loader.js:1-29; RTC.js:11-39`).
- Implement numbered storage records and metadata, selection, save/delete/clear, directory/find/old/read/write/print/binary operations, multiple storage devices, and Auto Load (`Storage.js:38-65, 770-1050; TEKTRONIX4051.js:1250-1275`).
- Preserve single-file import/export, storage archive import/export, Flash Drive file sets, metadata parsed from filenames, drag/drop where supported, overwrite confirmations, and user-visible completion/error states (`Tektronix4051.html:204-257; Storage.js:200-230, 265-395, 420-541, 1333-1428`).
- Test byte-for-byte import/export round trips and run the documented `FIND@5`, `OLD@5`, and `RUN` workflow against an existing supplied program (`README.md:67-74`).
- Make malformed archives, invalid filenames, unavailable files, canceled file pickers, and rejected overwrites explicit; do not disguise failures as successful loads.

*Exit gate:* the default Loader/RTC/tape devices work through emulated BASIC commands, storage operations preserve expected file numbers/headers/content, and supported import/export formats round-trip. Persistence across app restarts follows the Grade 0 decision, not an assumption.

= Grade 4 — Extended ROMs and remaining peripherals

*Goal:* cover the additional ROM-based programs and remaining user-visible peripheral behavior.

*Work:*

- Preserve all base and expanded ROM bytes and reproduce the option-ROM slot mapping (`Tek4051Rom.js:1-3, 262-4147; RomExp.js:10-13, 105-1137, 8416-9453, 15688-16724`).
- Execute representative programs that exercise each option-ROM family listed in the project README: R01, R05, R06, R07, R08, R12, R14, DDT51, Extended BASIC, and Extended Fonts (`README.md:115-127`).
- Add the GPIB Printer behavior and verify output from BASIC print operations. Replace browser popup output with a native egui window/panel or another agreed desktop presentation (`Printer.js:1-44; Storage.js:820-870`).
- Retain UI audio mute and beep/click behavior, but do not claim R12 MUSIC/SOUND support; the current project README marks those commands unsupported (`TEKTRONIX4051.js:153-157, 914, 1145-1166; README.md:86`).
- Exercise 1x through 100x speed settings, including stable user-visible responsiveness. Calibrate to the reference's benchmark behavior rather than using host CPU throughput as a substitute (`README.md:137-141; Tektronix4051.html:500-522; mc6800.js:60-66, 2409-2412`).

*Exit gate:* all documented 4051 option-ROM families are represented in executable test runs; printer and UI audio behavior match the accepted baseline; known unsupported BASIC sound features remain accurately described.

= Grade 5 — Full parity sign-off

*Goal:* demonstrate that the Rust application covers the agreed web feature set without silent omissions.

*Work:*

- Replay the Grade 0 interaction checklist from a clean application state, including boot, reset, keyboard and panel input, display modes, scaling, speed controls, file operations, Auto Load, GPIB devices, printer, audio, and supported option-ROM programs.
- Compare deterministic CPU/memory traces, logical display snapshots, GPIB byte transcripts, storage metadata/content, and visible status/error results against the web baseline. Use screenshot comparison only after matching logical screen coordinates and scaling.
- Test boundary and error paths: invalid/empty program input, missing tape files, invalid device addresses, malformed archive, canceled import/export, delete-all confirmation, and reset while running.
- Verify supplied assets were not modified and every imported/exported format remains usable with the documented 4050 Flash Drive files.
- Run Rust formatting, lint, build, and the focused/full Rust test suites selected by the eventual package. The current JavaScript `npm test` command is only a placeholder; it is not migration evidence (`package.json:6-11`).
- Keep the web emulator available as a reference until all sign-off evidence is recorded. Removal or cleanup of the legacy project is outside this port request.

*Exit gate:* every agreed feature has recorded evidence, all focused tests and builds pass, known gaps are explicitly listed, and the target has no unresolved behavior choices that affect user-visible parity.

= Cross-grade rules

- A feature does not count as migrated because a corresponding Rust type or UI widget exists; its observable behavior must pass its grade's exit gate.
- Preserve ROM, program, and other supplied data as compatibility inputs. Do not reformat, regenerate, or replace inherited assets as part of porting.
- Keep CPU, machine, display, GPIB, storage, and keyboard contracts testable without egui. Keep egui-specific code in the application adapter.
- Make errors observable. File and device failures must not silently return empty data or leave the UI in a success state.
- Report deviations from the reference intentionally. If a baseline mismatch is corrected, distinguish the fix from feature parity and retain evidence of the original behavior.
