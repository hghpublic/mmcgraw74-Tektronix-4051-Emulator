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
  Updated: 17 November 2026
]

#outline(title: [Contents], depth: 2)

= How to use the grades

The grades are ordered completion gates, not percentages or invented quality scores. Work may proceed across grades in small increments, but later implementation does not waive earlier evidence or imply that an earlier gate has passed. Keep the JavaScript emulator and its supplied ROM/program assets as the behavior oracle. Do not change inherited source files as part of this migration unless separately authorized.

Architectural refactoring is a separate track in `graded-architectural-refactoring-guide.typ`, with its own AR-0 through AR-5 gates. It preserves current Rust behavior while improving ownership and dependencies; passing an AR grade does not advance a migration grade, and incomplete migration parity does not itself block refactoring. Record any concrete cross-track prerequisite explicitly rather than merging their acceptance criteria.

The web entry point wires the visible controls to global JavaScript modules; the Rust implementation should reproduce those contracts and replace browser-specific interactions with egui/native adapters (`Tektronix4051.html:8-22, 37-58, 186-257, 300-318`). Native desktop and eframe are selected. Tape storage is intended to match the reference's in-memory behavior: the baseline observed saved files disappearing on page reload, despite the README's browser-local-storage claim. The design document at `design.typ` describes the current implementation and target boundaries.

== Current assessment

As of 17 November 2026, no migration grade exit gate has been formally accepted. The Grade 0 baseline remains draft and unfrozen. Since the assessment recorded on 14 November, implementation and test evidence has advanced substantially: the native application has broader tape and Flash Drive workflows, and the shared Rust session now also serves the WASM browser shell. Recorded package/WASM tests, strict lint, formatting, build, and focused browser checks establish implementation evidence, not completion of the full migration gates.

- *Grade 0 — open:* the reference revision and representative workflows are recorded, but the agreed environment mode and baseline bundle are not frozen. Character-size rendering, physical host-key behavior, printer output, and broader GPIB byte/EOI transcripts still need baseline evidence.
- *Grade 1 — open:* the machine executes the base ROM and tested BASIC workflows; CPU, memory, PIA, interrupt, keyboard, display, ROM mapping, and selected GPIB paths have focused coverage. Full instruction/board coverage and the planned differential traces remain incomplete.
- *Grade 2 — open:* the native egui shell implements the logical display and front-panel, keyboard, lifecycle, speed, and screen-copy workflows, with headless regression coverage. Physical native input and presentation, native dialog interaction, and editor/program-control acceptance remain unverified; the German Option+L mismatch is still open.
- *Grade 3 — open:* the machine implements the GPIB board, Loader/RTC, and a substantially expanded tape command set, including ASCII/binary transfers and LIST/header behavior. Native Flash Drive work includes loose-file/ZIP import, numbered-file export, whole-store archive restore/export, Auto Load, and guarded directory launch; shared clear-store behavior and the confirmed clear control are covered by WASM/API/browser regressions. Configurable device coverage, remaining tape/storage behavior, physical native file workflows, and broader supplied-program runs remain open.
- *Grade 4 — open:* ROM images and bank mappings are present, and one 4051R05 binary-program save/reload/run workflow is tested. Broader option-ROM execution, printer output, audio behavior, and speed calibration remain open.
- *Grade 5 — open:* no complete checklist replay or full parity sign-off has been recorded; it depends on evidence and decisions from the preceding grades.

The native read-only Flash Drive directory now shows sorted canonical metadata without copying file contents or changing transfer state. Core/native tests cover SAVE/WRITE updates, snapshot/cursor/clear preservation, actual control rendering, live updates, markers, invalid UTF-16 and Auto Load ownership. Tab navigation during Auto Load and after closing a focused directory is tested; hidden-directory/no-native-focus Tab still reaches BASIC and cancels the macro as before. This does not complete Grade 3: physical interaction and remaining storage management/formats are still open.

Evidence and limitations are tracked in `dev/docs/tek4051/phase-0/baseline-summary.md`, `dev/src/4051-Emulator/rs/4051-Emulator-rs/tests/test-cases/test-cases.md`, the Rust package's `tests/` and separate private test files, and `dev/copilot/chats/tek4051/tek4051-handoff.md`. Historical observations in the draft baseline are snapshots, not current test-inventory claims.

= Grade 0 — Baseline and scope ready

*Goal:* establish which web behaviors actually run and define the parity contract before implementation.

*Work:*

- The draft baseline pins revision `b33e20fb3ef41a1db114b602a29a9ba8ce3fdb48`, records the launch procedure and captures startup, reset, settings, tape import, Auto Load, and a vector program. Complete the missing physical-key, character-size, printer, and GPIB transcript observations.
- Inventory the exact script dependency list in `Tektronix4051.html`. Exclude unused `.old`, `.new`, alternate module, and test files unless runtime evidence shows they are required.
- Build a behavior checklist from the documented 4051 hardware, option ROMs, GPIB tape features, scaling, pixel modes, CPU speed, and keyboard instructions (`README.md:37-58, 109-141`).
- Preserve the selected in-memory storage policy. The source README mentions browser-local storage (`web/4051-Emulator/README.md:22`), while `Storage.js` initializes an in-memory `fileStore` (`Storage.js:38-40, 1076-1210`) and the draft baseline observed files disappearing on page reload. Flag contrary runtime evidence without silently changing the policy.
- The target is native desktop. Keep file dialogs, downloads, persistence, and printer presentation in native adapters.
- Preserve the source tree and asset files. Record representative program inputs and expected CPU, display, GPIB, and storage outcomes.
- Reconcile the draft baseline with the newer root Playwright suites and record their coverage, skips, known reference limitations, and provenance; do not treat skipped or partial scenarios as passing parity evidence. Confirm the deferred environment-mode choice, resolve remaining baseline gaps, and freeze the agreed bundle.

*Exit gate:* an executable reference baseline, an agreed feature checklist, target-platform decision, and explicit answers for persistence and known unsupported behavior. If the web reference cannot run reliably, mark affected behaviors “baseline blocked” rather than guessing expected output.

= Grade 1 — Machine core boots deterministically

*Goal:* create a UI-independent Rust machine core that can reset and execute the existing base system.

*Work:*

- Implement MC6800 state, instruction execution, flags, addressing, reset/interrupt vectors, and memory access; use fixed-width integers and checked boundaries.
- The repository-root `Cargo.toml` is a virtual workspace; its member package and library live under the Rust implementation root. Continue CPU/addressing coverage beyond the implemented subset and compare representative instruction and boot traces to the reference.
- The machine maps RAM, base and overflow ROMs, backpack and expander payloads, and PIA registers; tests exercise interrupt and keyboard signaling, display-ready/erase behavior, ROM bank boundaries, and selected GPIB handshake paths. Complete board behavior remains open (`TEKTRONIX4051.js:489-803, 1126-1209`).
- Preserve the recorded bank deviations: selectors 2/3 normalize uncovered JavaScript values to zero within the bank window, while the uncovered selector-1 and selector-4 tails return explicit errors. Establish any further compatibility consequences with reference evidence.
- Preserve the base ROM bytes and test byte count and digest against the reviewed source (`Tek4051Rom.js:1-3, 5960-5982`). Do not transcribe or “clean up” ROM contents.
- Use a deterministic machine-step API that accepts input and returns observable effects; avoid browser globals, wall-clock scheduling, and egui dependencies in this layer.
- Extend instruction-level tests and establish differential traces against the JavaScript reference for reset, representative opcodes/addressing modes, interrupts, and memory-mapped register access. Retain the bounded startup and `10 PRINT 1` / `RUN` regression without presenting their coverage as complete boot parity.

*Exit gate:* the Rust machine resets to the reference reset vector, matches selected deterministic CPU/memory traces, and executes the base ROM without a panic or state divergence in the covered cases. The core contract must remain independently testable as the UI evolves.

= Grade 2 — Interactive display and front panel

*Goal:* make the machine usable through an egui screen and keyboard/front-panel controls.

*Work:*

- Build on the existing UI-independent display model, `HostRuntime`, and eframe framebuffer presentation. The native window has been launched and an earlier shell screenshot observed, but physical interaction and presentation parity are not established.
- Smoke-test physical host keyboard input, the user-key, line-editor, and program buttons, Shift functions, and speed selection in the native window. Add executable editor and program-control behavior checks through the running ROM; PIA-code tests alone do not prove these workflows.
- Reproduce Tektronix key translation, modifiers, key press/release behavior, function keys, HOME/PAGE, RUB OUT, BREAK, and the clickable front-panel keys (`TekKeyboard.js:11-93, 279-365; Tektronix4051.html:72-192`).
- The current pointer adapter queues short presses and releases with 100 ms minimum intervals, unlike the web's immediate transitions. Retain this documented deviation and test its effects on real interaction; verify physical Caps Lock key behavior and implement the remaining controls.
- Render the logical vector display and character dots with matching coordinates, colors, dot sizing, cursor behavior, erase timing, persistence, and refresh decay (`TekDisplay.js:63-145, 159-241, 272-329, 342-365`).
- Keep logical plotting coordinates separate from egui scaling and window layout. Match the reference coarse, mixed, and fine display modes and Auto-Scaling behavior (`Tektronix4051.html:40-50, 465-498, 524-547`).
- Implement UI beep/click mute behavior and verify native status-light presentation. Reset restarts the CPU and requests a display erase; Start/Pause freeze CPU, keyboard, and phosphor intervals and restart them at fresh boundaries. Each independent 500 ms erase timeout must survive pause and reset, like the web's `setTimeout`. Verify both screen-copy triggers and native Save As interaction; the native modal dialog pauses interval work, unlike the web download, but its elapsed time still expires erase timeouts on close. Schedule bounded CPU work from elapsed time; preserve the reference's nominal interval and speed-factor behavior (`TEKTRONIX4051.js:1201-1209, 1277-1318; TekDisplay.js:272-303; mc6800.js:60-66, 326-331`).
- Compare logical framebuffer output first, then verify scaled-window presentation at varied aspect ratios.

*Exit gate:* a user can boot, type, use panel keys, interrupt/reset, and observe a representative BASIC character-and-vector workload. Key maps, pixel modes, logical screen snapshots, screen copy, and start/pause/reset controls have focused checks.

= Grade 3 — GPIB and mass-storage workflows

*Goal:* make BASIC programs load, communicate with peripherals, and use tape storage end to end.

*Work:*

- Start from the captured default device registry and tape Auto Load reference flow; implement the bus and storage path before claiming programs can be loaded from a flash-drive file set.
- Extend the implemented GPIB board handshake and Loader routing with address validation and device registration/removal (`GPIB.js:54-177, 194-287; TEKTRONIX4051.js:288-488`). Focused tests cover Loader PIA byte/EOI transcripts and an `OLD@1:` workflow; broader transaction coverage remains open.
- Extend the partial default 4924 tape drive beyond its numbered files/headers, FIND, SAVE, OLD, CLOSE, PRINT, INPUT, READ, WRITE, BSAVE/BOLD, LIST read/write, and directory subset. Preserve PRINT's current-file contents separately from the FIND snapshot and its whole-resulting-buffer cursor increment. Preserve INPUT's CR-terminated records and unchanged shared input at EOF or an unterminated tail. Preserve binary WRITE's per-byte commit and retained clear flag, READ's one-shot current-file response, the ASCII/binary type guards, BOLD's padded binary-program snapshot (including the extra aligned block), and LIST's current-file fixed-width fields and header-parser quirks. Keep the current header number separate from the FIND/fileNum control used by SAVE/BSAVE; marker FINDs retain snapshots/cursors, and LOG/TEXT transfers preserve usage. Malformed headers are an explicit documented Rust error instead of silent web rejection. Preserve the RTC's formatted local time and the machine's GPIB transaction ordering (`Tektronix4051.html:304-318; Storage.js:820-1050, 1270-1428; Loader.js:1-29; RTC.js:11-39`).
- Implement numbered storage records and metadata, selection, save/delete/clear, directory/find/old/read/write/print/binary operations, multiple storage devices, and Auto Load (`Storage.js:38-65, 770-1050; TEKTRONIX4051.js:1250-1275`).
- Preserve single-file import/export, storage archive import/export, Flash Drive file sets, metadata parsed from filenames, drag/drop where supported, overwrite confirmations, and user-visible completion/error states (`Tektronix4051.html:204-257; Storage.js:200-230, 265-395, 420-541, 1333-1428`).
- Test byte-for-byte import/export round trips and run the documented `FIND@5`, `OLD@5`, and `RUN` workflow against an existing supplied program (`README.md:67-74`).
- Make malformed archives, invalid filenames, unavailable files, canceled file pickers, and rejected overwrites explicit; do not disguise failures as successful loads.

*Exit gate:* the default Loader/RTC/tape devices work through emulated BASIC commands, storage operations preserve expected file numbers/headers/content, and supported import/export formats round-trip. Persistence across app restarts follows the Grade 0 decision, not an assumption.

= Grade 4 — Extended ROMs and remaining peripherals

*Goal:* cover the additional ROM-based programs and remaining user-visible peripheral behavior.

*Work:*

- Preserve all base and expanded ROM bytes and verify the existing option-ROM slot mapping against the reference (`Tek4051Rom.js:1-3, 262-4147; RomExp.js:10-13, 105-1137, 8416-9453, 15688-16724`). Mapped bytes and headers alone do not establish option-ROM program compatibility.
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
- Run Rust formatting, lint, build, and focused/full Rust tests. The repository-root `npm test` runs the web Playwright workspaces; record its results and skips as reference evidence, not as proof of Rust parity. The separate inherited web-source `package.json` still has a placeholder test command.
- Keep the web emulator available as a reference until all sign-off evidence is recorded. Removal or cleanup of the legacy project is outside this port request.

*Exit gate:* every agreed feature has recorded evidence, all focused tests and builds pass, known gaps are explicitly listed, and the target has no unresolved behavior choices that affect user-visible parity.

= Near-term sequence

1. Complete the missing Grade 0 observations, reconcile browser suites with the draft baseline, decide the environment mode, and freeze the agreed reference bundle only after its evidence is complete.
2. Verify native input and presentation, including physical keyboard/editor behavior, pointer timing, Save As dialogs, and the German Option+L case; distinguish headless tests from physical acceptance.
3. Close remaining tape/storage and configurable-device gaps, then compare supported supplied programs through native Auto Load and manual `FIND @5`, `OLD @5`, and `RUN` flows against the frozen reference.
4. Expand deterministic CPU/board differential evidence and representative option-ROM, printer, audio, and speed checks. Reassess every grade against its exit gate; implementation progress does not automatically pass a grade.

= Cross-grade rules

- A feature does not count as migrated because a corresponding Rust type or UI widget exists; its observable behavior must pass its grade's exit gate.
- Preserve ROM, program, and other supplied data as compatibility inputs. Do not reformat, regenerate, or replace inherited assets as part of porting.
- Keep CPU, machine, display, GPIB, storage, and keyboard contracts testable without egui. Keep egui-specific code in the application adapter.
- Make errors observable. File and device failures must not silently return empty data or leave the UI in a success state.
- Report deviations from the reference intentionally. If a baseline mismatch is corrected, distinguish the fix from feature parity and retain evidence of the original behavior.
