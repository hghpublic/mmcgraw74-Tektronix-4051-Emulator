# Tektronix 4051 Emulator Baseline

> Draft behavioral specification for the Rust migration. This file is not frozen: vector rendering is captured under all three display-granularity settings, but character-size rendering, printer output, reliable physical-keyboard input, and GPIB byte/EOI transcripts remain unverified. No test code is generated here.

## Metadata

| Field | Value |
|-------|-------|
| Project | Tektronix 4051 Emulator |
| Module | `tek4051-emulator` |
| Migration Scope | Port the existing browser-based Tektronix 4051 emulator to native Rust/egui while preserving observed 4051 behavior and documented limitations. |
| Created At | 2026-11-12 |
| Status | baseline draft (not frozen) |
| Testing Conventions | No existing web integration suite. Rust tests are inline `#[cfg(test)]` module tests. Future end-to-end checks should use Cargo's conventional `tests/` root and verify externally observable UI/display behavior. |

## Reference Source and Launch

- Source revision: `b33e20fb3ef41a1db114b602a29a9ba8ce3fdb48`
- Reference directory: `dev/src/4051-Emulator/web/4051-Emulator`
- Entry page SHA-256: `34a475e5bf001766816f32a0fad447c9a1d7190089ef58a9dfe48c0cfd44f6d9`
- Run from the repository root:

  ```sh
  python3 -m http.server 8765 --directory dev/src/4051-Emulator/web/4051-Emulator
  ```

- Open `http://127.0.0.1:8765/Tektronix4051.html`.
- The baseline program fixture `testdata/inputs/prog.txt` is byte-identical to the supplied `prog.txt` (SHA-256 `f472fc9080151e23d012fd0d1ace2e16904730eb57dc3d9f72a027e144d3b6d1`).

## Existing Test Coverage

No web integration, E2E, or regression tests were found. `npm test` exits 1 with `Error: no test specified`. The Rust package has 39 unit tests in its source modules, but no integration-test source tree existed before this baseline.

## Entry-Point Inventory

| Entry Point | Type | Source | Covered by |
|---|---|---|---|
| Browser load of `Tektronix4051.html`; `body.onload` invokes `tek_init(); updateScale()` | UI event | `Tektronix4051.html` | TC-EMU-001 |
| `#start.onclick -> tek_start()`, `#stop.onclick -> tek_stop()`, `#reset.onclick -> tek_reset()` | UI event | `Tektronix4051.html` | TC-EMU-002 |
| `toggleCPUspeed()`, `toggleScaling()`, `toggleDisplayGranularity()`, `toggleAudioStatus()` | UI event | `Tektronix4051.html` | TC-EMU-003, TC-EMU-010 |
| `#gpibdlg.onclick -> gpib.showGpibDialog()`, Add -> `gpib.deviceConnect()`, Remove -> `gpib.deviceRemove()` | UI event | `Tektronix4051.html`, `GPIB.js` | TC-EMU-004, TC-EMU-005 |
| `#importObj.onchange -> storage.importObject(storage)` | UI event | `Tektronix4051.html`, `Storage.js` | TC-EMU-006, TC-EMU-009 |
| `#fileNum`/Save -> `storage.saveFile()`, Select -> `storage.selectTekFile()` | UI event | `Tektronix4051.html`, `Storage.js` | TC-EMU-006, TC-EMU-008 |
| AUTO LOAD key -> `tek.autoload()` | UI event | `Tektronix4051.html`, `TEKTRONIX4051.js` | TC-EMU-006, TC-EMU-007, TC-EMU-010 |

The generic integration-test failure-coverage rule does not meaningfully apply to page startup, reset, or deterministic setting toggles: these actions have no external service dependency or application-level dependency-failure response. Relevant negative/no-op behavior is captured for duplicate GPIB addresses and empty/missing program files. This is one reason the baseline remains a draft.

## Test Cases

### TC-EMU-001 — Load the reference page

| Field | Value |
|-------|-------|
| ID | TC-EMU-001 |
| Category | happy-path |
| Entry Point Type | UI event |
| Entry Point | Browser load of `Tektronix4051.html`; `body.onload` invokes `tek_init(); updateScale()` |
| Description | The reference page initializes an idle 4051 front panel. |

**Trigger**

Serve the pinned source directory using the launch command above and navigate to `http://127.0.0.1:8765/Tektronix4051.html`.

**Preconditions**

- The static server serves the source revision recorded above.
- None (no emulator state is required before page load).

**Expected Response**

- Page title: `jsTEKTRONIX4051 - A Tektronix 4051 emulator written in Javascript. Copyright (c) 2015 David E. Roberts.`
- `#start` is enabled; `#stop`, `#gpibdlg`, and `#storagedlg` are disabled.
- Speed is `x1`; scaling is on (`⇲`); display granularity control is `⊚`; audio control is `♫ mute`.

**Resource Verification**

- None. This is a client-side emulator with no external service or persistent resource.

**Negative Verification**

- No child window or modal dialog is opened by page initialization.

**Data References**

- None.

### TC-EMU-002 — Start, pause, and reset

| Field | Value |
|-------|-------|
| ID | TC-EMU-002 |
| Category | happy-path |
| Entry Point Type | UI event |
| Entry Point | `#start.onclick -> tek_start()`, `#stop.onclick -> tek_stop()`, `#reset.onclick -> tek_reset()` |
| Description | Front-panel lifecycle controls update enabled state, power indication, and display contents. |

**Trigger**

From a freshly loaded page, click Start, wait for the UI state to settle, click Pause, then click Reset.

**Preconditions**

- TC-EMU-001 completed.

**Expected Response**

- After Start: `#start` disabled, `#stop`, `#gpibdlg`, and `#storagedlg` enabled, and power indicator color `rgb(0, 238, 0)`.
- After Pause: `#start` enabled, `#stop`, `#gpibdlg`, and `#storagedlg` disabled, and power indicator color `rgb(0, 85, 0)`.
- After Reset while paused: the 1046×830 display canvas contains 0 non-black pixels.

**Resource Verification**

- None. The observable state is limited to front-panel controls and the canvas.

**Data References**

- None.

### TC-EMU-003 — Cycle front-panel settings

| Field | Value |
|-------|-------|
| ID | TC-EMU-003 |
| Category | boundary |
| Entry Point Type | UI event |
| Entry Point | `toggleCPUspeed()`, `toggleScaling()`, `toggleDisplayGranularity()`, `toggleAudioStatus()` |
| Description | Repeated setting clicks cycle through the reference's supported control values and wrap to the initial value. |

**Trigger**

Click the speed control seven times, scaling twice, display granularity three times, and audio mute twice from a fresh page.

**Preconditions**

- TC-EMU-001 completed.

**Expected Response**

- The exact observed sequences are in `testdata/expectations/control-cycles.json`.

**Resource Verification**

- None. These controls update in-process emulator/UI state.

**Data References**

- `testdata/expectations/control-cycles.json`

### TC-EMU-004 — Add and remove a GPIB printer

| Field | Value |
|-------|-------|
| ID | TC-EMU-004 |
| Category | happy-path |
| Entry Point Type | UI event |
| Entry Point | `gpib.showGpibDialog()`, `gpib.deviceConnect()`, and `gpib.deviceRemove()` |
| Description | The GPIB dialog displays default devices and permits adding then removing a printer at an unused address. |

**Trigger**

Start the emulator, open the GPIB dialog, select address `6` and device `Printer`, click Add, then select address `6` and click Remove.

**Preconditions**

- TC-EMU-001 completed; emulator is running.
- Default devices are registered at addresses 1, 2, and 5.

**Expected Response**

- Initial list: `1: Program loader`, `2: Real time clock`, `5: 4924 tape drive`.
- After Add: the list contains the three defaults plus `6: Printer`.
- After Remove: the list returns to exactly the three default devices.

**Resource Verification**

- The in-process GPIB device registry, as shown in the dialog's `#deviceViewer`, has the exact address/description entries above.

**Data References**

- None.

### TC-EMU-005 — Ignore a duplicate GPIB address

| Field | Value |
|-------|-------|
| ID | TC-EMU-005 |
| Category | failure |
| Entry Point Type | UI event |
| Entry Point | `gpib.deviceConnect()` |
| Description | Adding another device at an occupied address leaves the registry unchanged. |

**Trigger**

Start the emulator, open the GPIB dialog, select address `1`, select `Printer`, and click Add.

**Preconditions**

- The default device registry is present: address 1 `Program loader`, address 2 `Real time clock`, address 5 `4924 tape drive`.

**Expected Response**

- `#deviceViewer` remains exactly:

  ```text
  1: Program loader
  2: Real time clock
  5: 4924 tape drive
  ```

- No error or confirmation message is displayed.

**Resource Verification**

- The in-process GPIB registry contains exactly the same three devices after the attempted addition.

**Negative Verification**

- No second device is registered at address 1.
- No `Printer` entry is added to `#deviceViewer`.

**Data References**

- None.

### TC-EMU-006 — Import, select, and Auto Load the vector program

| Field | Value |
|-------|-------|
| ID | TC-EMU-006 |
| Category | happy-path |
| Entry Point Type | UI event |
| Entry Point | `storage.importObject(storage)`, `storage.saveFile()`, `storage.selectTekFile()`, and `tek.autoload()` |
| Description | A supplied BASIC program is stored as tape file 1, transferred to the program loader, and executed by Auto Load. |

**Trigger**

Start the emulator and open Storage. Import `testdata/inputs/prog.txt` as a single file. Set file number to `1`, Type to `Ascii`, Usage to `Program`, and Description to `Vector graphics demo`; click Save and then Select. Click the AUTO LOAD key and wait until the drawing is stable.

**Preconditions**

- Fresh page state with no tape files.
- The source and fixture hashes match the metadata above.
- `testdata/inputs/prog.txt` is selected through the browser file picker.

**Expected Response**

- Storage shows tape file 1 with type `ASCII`, usage `PROG`, description `Vector graphics demo`, and size `235`.
- The display capture matches `testdata/expectations/vector-program-display.json`.
- Auto Load sends the reference command sequence `FIND @5:1`, `OLD @5:`, `RUN`.

**Resource Verification**

- The in-memory tape store contains file number 1 with the imported program bytes and the selected metadata.
- The visible display canvas has the exact dimensions, green-pixel counts, bounds, and color counts in `testdata/expectations/vector-program-display.json`.

**Data References**

- `testdata/inputs/prog.txt`
- `testdata/expectations/vector-program-display.json`

### TC-EMU-007 — Auto Load with no tape program

| Field | Value |
|-------|-------|
| ID | TC-EMU-007 |
| Category | failure |
| Entry Point Type | UI event |
| Entry Point | `tek.autoload()` |
| Description | Auto Load against an empty tape store reports the reference's missing-program message and does not draw the sample figure. |

**Trigger**

With an empty tape store, start the emulator and click the AUTO LOAD key. Observe the display immediately after the command sequence completes.

**Preconditions**

- The emulator has just been loaded or reloaded; tape file list is empty.
- No program has been uploaded to the program loader.

**Expected Response**

- The display shows the command echo and message in `testdata/expectations/no-program-autoload.json`.
- The exact error text is `NO PROGRAM FOUND - MESSAGE NUMBER 59`.
- The emulator remains responsive; the display may later clear as the reference continues running.

**Resource Verification**

- The in-memory tape store remains empty.

**Negative Verification**

- The vector figure from `testdata/inputs/prog.txt` is not drawn.
- No tape file is created by the failed Auto Load sequence.

**Data References**

- `testdata/inputs/prog.txt`
- `testdata/expectations/no-program-autoload.json`

### TC-EMU-008 — Confirm tape storage is in-memory

| Field | Value |
|-------|-------|
| ID | TC-EMU-008 |
| Category | boundary |
| Entry Point Type | UI event |
| Entry Point | `storage.saveFile()` followed by browser reload and `storage.showStorageDialog()` |
| Description | A saved tape file is available during the page lifetime and is absent after reloading the page. |

**Trigger**

Import `testdata/inputs/prog.txt`, save it as file 1, reload the browser page, start the emulator, and open Storage.

**Preconditions**

- Fresh page state.
- The in-memory store contains file 1 immediately before reload.

**Expected Response**

- Before reload, the file selector contains exactly `1`.
- After reload, the file selector contains no entries.

**Resource Verification**

- The in-memory tape store is empty after reload; no browser-persistent tape resource is present.

**Data References**

- `testdata/inputs/prog.txt`

### TC-EMU-009 — Import an empty file

| Field | Value |
|-------|-------|
| ID | TC-EMU-009 |
| Category | failure |
| Entry Point Type | UI event |
| Entry Point | `storage.importObject(storage)` via `#importObj.onchange` |
| Description | Importing an empty file does not create a tape entry and exposes the current reference's empty-file behavior. |

**Trigger**

Open Storage and import `testdata/inputs/empty.txt` using the Single file option.

**Preconditions**

- Fresh page state with an empty tape store.

**Expected Response**

- The filename label is `empty.txt`, the viewer is blank, the file selector has no entries, and the storage notification is not visible.
- No explicit import error is displayed; this is the current reference behavior, not a recommended Rust UX.

**Resource Verification**

- The in-memory tape store remains empty.

**Negative Verification**

- No tape file is created.
- No program bytes are transferred to the loader.

**Data References**

- `testdata/inputs/empty.txt`

### TC-EMU-010 — Compare vector rendering across display-granularity settings

| Field | Value |
|-------|-------|
| ID | TC-EMU-010 |
| Category | boundary |
| Entry Point Type | UI event |
| Entry Point | `toggleDisplayGranularity()` and `tek.autoload()` |
| Description | The same vector program renders with the reference's three display-granularity settings. |

**Trigger**

Import `testdata/inputs/prog.txt` as ASCII/Program file 1 with description `Vector graphics demo`. Set CPU speed to `x100`. For each display control value `⊚`, `⊙`, and `⊛`, start the emulator, click AUTO LOAD, wait 20 seconds, pause, and record the raw RGB canvas counts. Between runs, reset, reselect file 1 in Storage, and advance the granularity control to the next value.

**Preconditions**

- A fresh page with the supplied program saved as tape file 1.
- The display-granularity control starts at `⊚`.

**Expected Response**

- For each control value, the 1046×830 canvas pixel counts match `testdata/expectations/display-granularity.json`.
- No assertion is made about canvas bounds because command echo and cursor pixels vary between captures.

**Resource Verification**

- The display canvas has exactly the per-color pixel counts recorded for each granularity value in `testdata/expectations/display-granularity.json`.

**Data References**

- `testdata/inputs/prog.txt`
- `testdata/expectations/display-granularity.json`

## Freeze Gate

- [x] Test case IDs are unique and use `TC-EMU-NNN`.
- [x] Each case has a category, concrete trigger, preconditions, expected response, resource verification, and applicable negative verification.
- [x] All referenced fixture files exist.
- [x] Vector rendering is captured under all three display-granularity settings.
- [ ] Full entry-point happy/failure coverage is not yet demonstrated for deterministic UI controls with no failure mapping.
- [ ] Character-size rendering, printer output, reliable physical host-key input, and GPIB byte/EOI transcripts remain unverified.
- [ ] Environment-mode metadata is provisional (`mock`, no external dependencies).
- [ ] Therefore this bundle is not frozen and must not be used to claim complete Grade 0 parity.
