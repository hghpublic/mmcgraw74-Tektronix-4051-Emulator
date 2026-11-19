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
  #text(size: 20pt, weight: "bold")[Tektronix 4051]
  \
  #text(size: 15pt)[BASIC: A Starting Guide]
]

#align(center)[Updated: 15 November 2026]

#outline(title: [Contents], depth: 2)

= Getting started

This guide introduces a small selection of Tektronix 4051 BASIC commands and statements. It is not a complete language reference. The examples describe 4051 BASIC; they do not imply that every feature has been verified in the Rust emulator.

Enter each line and press RETURN. A line beginning with a number is stored as part of the program; an unnumbered command such as `RUN` is entered directly. For example:

```text
10 PRINT 1
RUN
```

The program prints `1`. Entering another statement with an existing line number replaces that line. In the native Rust port, this small program has been exercised through the machine/runtime API; physical keyboard input remains unverified.

== Native execution and responsiveness

Select *Start* to run and *Pause* to stop interval execution. The native window presents immutable snapshots from a separate emulation worker; it does not execute CPU catch-up or wait for that worker during a repaint. Controls submit ordered requests, so their effect appears after the worker processes them. The last display and directory remain available while the worker is busy. Worker failures are reported in the window.

The CPU-speed button cycles from `x1` through `x100`. These are requested speeds, not a guarantee of real-time throughput. When execution falls behind, the native worker discards excess catch-up time instead of accumulating an ever-growing backlog; a yellow timing notice reports the skipped duration. Reduce the requested speed if that value keeps increasing. Display erase deadlines still account for elapsed wall time.

File choosers, host reads, archive processing and image/file writes run outside the repaint path. Only one file operation is offered at a time. The chooser waits asynchronously for the worker to acknowledge suspended execution; export names and subsequent file/archive writes then use that frozen store. As before, modal file operations pause CPU, keyboard and display-refresh intervals; erase completion is retained. Pause and Reset do not block the window while waiting for the worker, but an active worker-side read/write must return before the worker can apply them. Closing the window requests shutdown without joining the worker inside the event loop.

= Commands and statements

== PRINT

`PRINT` displays a value or text. Text literals are enclosed in quotation marks.

```text
10 PRINT "HELLO"
20 PRINT 2+3
```

== Assignment

An assignment stores a value in a variable. This example assigns `5` to `N` and prints it:

```text
10 N=5
20 PRINT N
```

== DIM and INPUT

`DIM` specifies storage for an array or string. `INPUT` reads a value entered by the user. Here `N$` is a string variable:

```text
10 DIM N$(20)
20 INPUT N$
30 PRINT N$
```

== LIST

`LIST` displays the program currently stored in BASIC memory. Use it after entering numbered lines or loading a program with `OLD`:

```text
LIST
```

This lists the program; it does not execute it or read tape-file metadata. The tape LIST operations described below use a GPIB secondary address and are separate from this command.

== RUN

`RUN` executes the program currently stored in BASIC memory:

```text
RUN
```

== REM and END

`REM` introduces a remark in a program. `END` ends program execution.

```text
10 REM PRINT A VALUE
20 PRINT 5
30 END
```

== IF and GO TO

`IF` tests a condition; when it is true, `THEN` can transfer execution to a line number. `GO TO` transfers execution directly.

```text
10 N=1
20 PRINT N
30 N=N+1
40 IF N<4 THEN 20
50 GO TO 70
60 PRINT "SKIPPED"
70 END
```

This prints `1`, `2`, and `3`, then skips line `60`.

== FOR and NEXT

`FOR` and `NEXT` repeat a group of statements while advancing a loop variable:

```text
10 FOR I=1 TO 3
20 PRINT I
30 NEXT I
40 END
```

== GOSUB and RETURN

`GOSUB` transfers control to a subroutine. `RETURN` continues after the `GOSUB` that called it.

```text
10 GOSUB 100
20 END
100 PRINT "HELLO"
110 RETURN
```

== CALL

`CALL` invokes a named routine supplied by an installed option ROM. Put the routine name in quotation marks and follow it with any required arguments, separated by commas. The arguments depend on the routine; unlike `GOSUB`, `CALL` does not branch to a numbered line in your BASIC program.

```text
CALL "BSAVE",5
```

This invokes the 4051R05 Binary Program Loader's `BSAVE` routine. For this routine, `5` is the GPIB device address; select the tape file with `FIND` first. The tape examples below explain `BSAVE` and `BOLD`. The emulator includes the R05 ROM and has a tested save/reload workflow, but this does not establish support for every option-ROM routine.

== PAGE, MOVE, and DRAW

`PAGE` clears the display. `MOVE` positions the graphics cursor; `DRAW` draws from the current position to the specified coordinates.

```text
10 PAGE
20 MOVE 10,10
30 DRAW 90,10
40 END
```

= Programs on GPIB tape

In the emulator's default device configuration, address `5` is the tape device.

== Importing Flash Drive files

The native Rust app's *Import folder contents...* button opens a folder chooser. Every regular file directly in the selected folder is considered for import; subfolders are not scanned. Files with 4050 Flash Drive-style filenames are accepted, for example:

```text
1      ASCII   PROG [Demo] 12.txt
```

The filename supplies the number, type, usage, and description using the LIST header parser described below. Contents are read as unchanged bytes; the size in the filename does not resize them. ZIP bundles of these numbered files are also accepted, using each entry's basename without extracting files to disk. This merge mode does not accept an ordinary unmarked text file or the whole-storage archive format described below. If a folder contains no files, the app reports that instead of treating it as a successful import.

Valid files are merged into the in-memory tape. Matching numbers are overwritten in sorted path order, so keep any existing tape contents you need before importing. Other stored files remain. Each invalid filename or host read failure is reported without discarding successful imports; cancelling the chooser imports nothing. File numbers must be 1 through 254.

Import selects the first number in the whole stored tape, not necessarily the first newly imported file. It does not rewind the FIND snapshot or cursor, load BASIC memory, or start execution. If the app is paused, select Start, then explicitly FIND the desired ASCII program before OLD and RUN:

```text
FIND@5:1
OLD@5:
RUN
```

A Rust ROM/runtime test imports a small ASCII program and verifies this load/run path. Headless native tests cover the import button, errors, cancellation, and modal timing; actual native chooser and physical keyboard interaction remain unverified. Plain unmarked-file import remains unimplemented. Imported copies and later changes are held in memory; original host files are not modified.

== Viewing the native Flash Drive directory

Select *Show directory* to open a side panel. It lists stored file numbers in ascending order, with type, usage, byte count and description. Resize the panel or scroll when needed; clear the checkbox to restore the display area. Start the emulator, wait until BASIC displays its prompt, and check *I see the BASIC prompt*. Then double-click an ASCII PROG file number to launch it, or select a row and use Up/Down followed by Return. Only ASCII PROG entries can be launched this way; data, binary and marker entries remain visible but are not run as BASIC programs.

The directory shows canonical stored metadata, including the latest SAVE or data-write size, rather than the older working file reported by tape LIST. Viewing or selecting a row does not select a tape file, change the export number, rewind a transfer cursor, load BASIC memory or start/pause execution. Launching an ASCII PROG sends `FIND@5:n`, `OLD@5:`, and `RUN` through the emulated keyboard. The app cannot detect whether BASIC is at its prompt, so the checkbox is an explicit user confirmation; it is cleared when you pause, reset or encounter a runtime error. Launch commands are sent only while running and after confirmation, but the status does not guarantee that the program executed successfully. Entries refresh as stored files change.

When either native checkbox has keyboard focus, Space and Enter toggle only that control. When a file number has focus, Up/Down changes the selected row and Return requests launch of an ASCII PROG; these keys are owned by the directory rather than sent to BASIC. Paired text, repeats and releases remain owned after focus moves, so native activation cannot type into BASIC or cancel Auto Load. Owned activation repeats are consumed before native widgets run, preventing a held key from activating another control such as Reset.

Tab navigates native controls while the directory is open or a native control already has focus, including after closing the directory. Otherwise Tab is forwarded to BASIC and retains its normal manual-input cancellation of Auto Load. Native-owned Tab presses, repeats and releases do not interrupt Auto Load.

Description padding is omitted only for display. A split UTF-16 surrogate produces a visible description error instead of replacement characters or a hidden entry. NEW/LAST may have no usage, shown as a dash. Path-like descriptions can be inspected even when their filenames cannot be exported safely.

The launch notice confirms command delivery, not successful program execution; BASIC or model-specific limitations may still stop a program. Headless tests exercise directory selection and keyboard launch through the ROM. Physical native panel interaction remains unverified.

== Exporting one Flash Drive file

Set *Export file number* to a stored number from 1 through 254, then select *Export Flash Drive file...*. Native Save As proposes a numbered Flash Drive filename containing the stored type, usage, padded description and actual byte size. Keep that filename when saving if you want to import the file's metadata again. The destination receives unchanged raw bytes, not a text conversion or ZIP archive.

Export reads the stored entry, including the latest SAVE or data-write contents, rather than the older FIND/OLD snapshot or working file view. It does not select a file, rewind a cursor, load BASIC memory or start the machine. Editing the number owns native keyboard input and releases held emulated keys; it does not type digits or RETURN into BASIC. Export and its number editor are available while paused or after a runtime error, but disabled while Auto Load types.

Success, cancellation, missing files and write failures are reported in the window. Descriptions containing path separators, NUL or a split UTF-16 surrogate are rejected instead of silently changing the filename. Numbered-file import still truncates descriptions to 16 UTF-16 units; whole-storage archive import preserves its validated metadata, including NEW/LAST usage labels. Native dialog/write time pauses CPU, keyboard and refresh intervals while pending erase completions continue.

Headless tests exercise the actual egui control with injected Save As outcomes, raw-byte file readback, metadata round trips, retained transfer state and modal timing. Actual native chooser interaction and overwrite confirmation remain unverified.

== Exporting a full Flash Drive archive

Select *Export Flash Drive archive...* to save the complete in-memory store as a ZIP file; Save As proposes `Archive.zip`. Entries are ordered by stored file number and alternate between a JSON metadata header and the unchanged file contents, matching the web emulator's whole-storage archive format. Export does not change stored files or tape transfer state. An empty store cannot be exported; cancellation and write failures are reported.

This whole-storage archive differs from a ZIP bundle of individual Flash Drive files, which the other Import control merges into the current store. Select *Import Flash Drive archive...* to replace the entire store; a native warning asks for confirmation before opening the ZIP chooser. The importer validates every alternating JSON-header/content pair before committing, so a malformed archive leaves current files intact. Duplicate file numbers use the last pair, and a valid empty ZIP clears storage after confirmation. A successful replacement clears the stale FIND/OLD snapshot, cursor and pending WRITE clear, and aborts any currently addressed tape transfer; a pending read receives the empty-input sentinel with EOI and partial output is discarded. Use FIND before OLD to read a replacement file.

Replacement archives are limited to 128 MiB compressed, 4,096 ZIP entries, 8 MiB expanded per entry and 64 MiB expanded total. Header descriptions retain their UTF-16 units, including escaped unpaired surrogates, and are limited to 16 units. Cancellation and validation failures are reported without changing stored files.

Headless tests read back the streamed file-backed ZIP and verify ordered metadata/content pairs, raw bytes and escaping. They also cover replacement, malformed-input atomicity, empty archives and the import control outcomes. Actual native import confirmation, chooser interaction and Save As overwrite behavior remain unverified.

== Native Auto Load control

*Auto Load* is a native host control, not a BASIC statement or a file-import operation. Import an ASCII program at file number `1`, select *Start*, and wait until BASIC is ready for commands. Auto Load then types the web reference's fixed sequence through the emulated keyboard:

```text
FIND@5:1
OLD@5:
RUN
```

It always targets file `1`, not the current selection or first newly imported file. It neither opens a chooser nor starts a paused emulator. To launch another ASCII PROG, use the directory's selection controls; otherwise enter FIND, OLD, and RUN yourself. Ensure the intended program is present: Auto Load does not validate the tape file or clear the reference's retained OLD snapshot after a missing FIND.

The native macro uses the existing 100 ms press and 100 ms release intervals with Shift and Control inactive, rather than the web's asynchronous key timing. A busy manual keyboard is rejected visibly. Duplicate Auto Load, file import and file export are disabled while it types. Pause, Reset, a runtime error, manual host/panel input, or loss of window focus cancels pending macro keys. Cancellation can leave a partial command in the BASIC editor; review that line before continuing.

The notice *Auto Load commands sent (file 1)* means key delivery finished, not that every program ran successfully. A headless native test clicks this control, runs an imported small program through the real ROM, and compares its complete output-pixel mask with independently entered BASIC. Actual native interaction and broader supplied programs remain unverified.

== FIND

`FIND` selects a numbered file on a device. In this example, `@5` identifies the tape device and `1` is the file number:

```text
FIND@5:1
```

`FIND` prepares the file for a subsequent operation; it does not load the program into BASIC memory. In the current emulators, selecting a missing in-range file can leave the previous tape buffer intact, so do not assume that `FIND` confirms the file exists.

== OLD

`OLD` loads the selected program from the addressed device into BASIC memory, replacing the current program. Save any program you want to keep before loading another.

```text
FIND@5:1
OLD@5:
RUN
```

Here `FIND` selects file `1`, `OLD` loads it, and `RUN` executes it.

== Saving a program

Select a numbered file before saving:

```text
FIND@5:1
SAVE@5:
```

This saves the current program to file `1` and can replace its previous contents. Select the file again before loading the saved version:

```text
FIND@5:1
OLD@5:
RUN
```

The web reference uses in-memory storage, so saved files do not persist across a page reload. The Rust port also keeps tape files in memory; use the native numbered-file export control to retain a host copy. Its machine/runtime tests cover the shown FIND, SAVE, OLD, and RUN sequence for a small BASIC program.

== Writing an ASCII data record

`PRINT` to the tape writes values or text, rather than saving the BASIC program itself. Select a numbered file first:

```text
FIND@5:1
PRINT@5:1
FIND@5:1
INPUT@5:N
PRINT N
```

The `1` after `PRINT@5:` is the value to write, not the file number. The first `PRINT` at record position zero replaces the selected file's contents; subsequent `PRINT` transfers append. Save any existing contents you need before overwriting them. Repeat `FIND` before reading the newly written bytes, because writes do not refresh the selected read snapshot. A Rust machine/runtime test exercises this numeric round-trip.

== Reading an ASCII data record

If tape file `1` contains a CR-terminated numeric ASCII record, this reads its value into `N` and prints it:

```text
FIND@5:1
INPUT@5:N
PRINT N
```

Each tape `INPUT` advances to the next CR-terminated record. The current emulators do not provide a fresh response at EOF or for an unterminated final record; the shared input buffer is left unchanged. A Rust machine/runtime test prepares file `1` through the API, reads `1` into a variable, and verifies the printed result. That test does not exercise the native file chooser.

== WRITE

`WRITE` sends a value in binary form to the selected tape file, rather than the ASCII records produced by tape `PRINT`. It writes data, not the BASIC program itself. Use a missing numbered file to create binary data; the web reference and Rust port ignore `WRITE` to an ASCII file and `PRINT` to a binary file.

```text
FIND@5:2
WRITE@5:1
```

Here `FIND` selects file `2`, and `WRITE` stores the value `1`. The value after `WRITE@5:` is not the file number.

Selecting an existing data file arms clearing of its working contents on the next `WRITE`; later bytes append, and each byte is committed without waiting for EOI. Keep any contents you need before rewriting.

== READ

`READ` retrieves binary data from the selected tape file into a variable. Unlike `OLD`, it does not load a BASIC program. After writing file `2` as above, reselect it, read its value into `N`, and display the result:

```text
FIND@5:2
READ@5:N
PRINT N
```

The `N` after `READ@5:` is the receiving variable, not the file number. The shown WRITE/FIND/READ sequence is covered by a Rust ROM/runtime numeric round-trip test.

Repeat `FIND` before rereading: the web handler supplies the whole current binary file only at shared cursor zero, and later `READ` calls do not refresh the shared input. This differs from ASCII `INPUT`, which walks CR-terminated records. Native Flash Drive file-set import is available; Auto Load's fixed OLD/RUN sequence is for a file-1 BASIC program, not this binary data workflow.

== BSAVE

The 4051R05 Binary Program Loader's `BSAVE` routine saves the current BASIC program in binary form, rather than the data values handled by `WRITE`. Invoke it through `CALL`, not as a standalone BASIC statement:

```text
10 PRINT 1
FIND@5:3
CALL "BSAVE",5
```

Here `FIND` selects file `3`, and the `5` after the quoted routine name is the GPIB device address. `BSAVE` replaces the selected stored file when EOI ends the transfer; keep any previous file contents you need before overwriting them. It does not update the selected read snapshot, so repeat `FIND` before loading the saved copy.

== BOLD

The R05 `BOLD` routine loads a selected binary program into BASIC memory. Unlike `READ`, it loads a program rather than a data value. It replaces the current program, so save any work you want to keep. After the BSAVE example above:

```text
10 PRINT 2
FIND@5:3
CALL "BOLD",5
RUN
```

This changes the program in memory, reselects the saved binary program at file `3`, then reloads it and runs it. The final output is `1`, not `2`. A Rust ROM/runtime test verifies the combined BSAVE/modify/BOLD/RUN workflow. As with `BSAVE`, the `5` is the GPIB address and `BOLD` is a quoted routine name, not a standalone BASIC statement. Select a binary program saved by `BSAVE`, not binary data written by `WRITE`.

The BOLD tape handler appends `0xff` and spaces through the next 256-byte boundary, including a full extra block for an already aligned program, matching the web reference. This tested workflow does not establish support for every R05 routine or general option-ROM compatibility.

== LIST: reading a tape entry

Tape LIST is a peripheral operation, distinct from the BASIC `LIST` command that displays the program. Read the selected file's entry with `INPUT` using secondary address `19`:

```text
10 DIM A$(60)
20 FIND@5:1
30 INPUT@5,19:A$
40 PRINT LEN(A$)
```

In `@5,19`, `5` is the GPIB device address and `19` selects the tape LIST read operation (bus command `0x73`). The `1` after `FIND@5:` is the file number; `A$` is the receiving string, not a file number. It receives these fields:

- File number, padded to 7 columns.
- ASCII/BINARY or NEW/LAST type, padded to 8 columns.
- PROG/DATA/LOG/TEXT usage, padded to 5 columns; markers may have a blank usage.
- Description, padded to 16 columns.
- One separator space followed by the current byte size in decimal, without padding.

There is no trailing CR or NUL in the response. A file with a one-digit byte count produces 38 characters, including padding spaces. A Rust ROM/runtime test reads the entry for an API-stored ASCII program and verifies that `LEN(A$)` prints `38`.

The entry describes the current working file, including any description set by a LIST header write or Flash Drive import. SAVE can change stored content without updating the working type, size, or description until another `FIND`. Missing or untyped selections return the reference's `0xff` sentinel. Auto Load does not change these LIST metadata rules.

== LIST: writing a tape header

Tape LIST header writes use `PRINT` at secondary address `19`, not the standalone BASIC `LIST` command. The following example requires file `1` to contain the bytes you want to mark or copy:

```text
10 DIM H$(60)
20 DIM A$(60)
30 H$="2      ASCII   TEXT [ROM] 999"
40 FIND@5:1
50 PRINT@5,19:H$
60 FIND@5:2
70 INPUT@5,19:A$
80 PRINT A$
```

At EOI, the header marks the working contents as ASCII/TEXT with description `ROM` and saves them under file number `2`. It does not delete file `1`. The final `999` does not resize the contents; LIST read reports their actual byte size. A Rust ROM/runtime test copies a 12-byte program this way and verifies the complete printed metadata entry.

Spacing matters. The parser reads a number from the first six characters, the type's initial at column 8, and, for ASCII/BINARY, the usage's initial at column 16. It removes brackets from the description between column 21 and the final space, then truncates or pads it to 16 characters. An arbitrary description string is not a valid header.

Keep any contents you need before changing a header: the operation saves the current working bytes, which can differ from the stored file. Header writes alone do not refresh the FIND snapshot, rewind data reads, or disarm a pending WRITE clear. Renumbering also leaves SAVE/BSAVE targeting the preceding FIND number; repeat `FIND` for the new number before using them, as the example does.

A numeric header number outside 1 through 254 leaves the previous current number unchanged; without a usable number, Rust reports an error rather than creating an unnumbered entry. Rust also reports malformed headers explicitly instead of the web's silent rejection.

= Tape usages and markers

LOG and TEXT are tape-header *usage* labels; NEW and LAST are tape-header *type* markers. They are not standalone BASIC commands. Set them through the LIST header-write operation described above.

The header assignment examples below replace line `30` in that example. They keep file number `2`, so its subsequent FIND and metadata read still apply. Changing a header saves the working bytes; keep any contents you need before doing so.

== LOG usage

`LOG` is a usage label that can accompany either ASCII or BINARY type. To copy the selected working contents to file `2` marked ASCII/LOG:

```text
30 H$="2      ASCII   LOG  [Notes] 0"
```

The two spaces after `LOG` complete the five-column usage field. Usage does not determine the data encoding: ASCII files use PRINT/INPUT, while binary files use WRITE/READ. These transfers preserve LOG usage; BOLD requires BINARY/PROG instead.

FIND of an ASCII/BINARY LOG file rewinds its data cursor but does not arm a new DATA-file WRITE clear. It does not cancel a clear already pending from an earlier DATA selection.

== TEXT usage

`TEXT` is another usage label, also valid with either ASCII or BINARY type. For an ASCII/TEXT entry:

```text
30 H$="2      ASCII   TEXT [Notes] 0"
```

The space after `TEXT` completes the usage field. TEXT does not convert the stored bytes to ASCII; the separate type field determines the applicable transfer commands. PRINT/WRITE preserve TEXT usage. As with LOG, FIND rewinds an ASCII/binary file without newly arming WRITE clearing, and a preceding pending clear remains pending.

Rust PIA tests cover both LOG and TEXT with ASCII and binary transfers. The complete LIST example above also has a real-ROM ASCII/TEXT header round-trip.

== NEW marker

`NEW` marks the file's type rather than its usage:

```text
30 H$="2      NEW          [Ready] 0"
```

The blank usage field is ignored, not used to clear any previous usage. Header writes retain existing working bytes, so marking a file NEW does not by itself empty it.

A subsequent tape PRINT changes NEW to ASCII/DATA; WRITE changes it to BINARY/DATA. The description remains. PRINT still replaces or appends according to the retained record cursor, and WRITE still consumes any pending clear. FIND of a NEW marker does not refresh the FIND snapshot or rewind that cursor, so do not treat it as a fresh data-file rewind.

== LAST marker

`LAST` is likewise a type marker:

```text
30 H$="2      LAST         [End] 0"
```

Its usage field is ignored and any preceding usage is retained. Marking LAST saves the current bytes and header; it does not delete the file or require its contents to be empty.

LAST ignores tape PRINT and WRITE commits. However, a pending WRITE clear occurs before type rejection: an ignored WRITE can clear working bytes without changing stored contents. FIND of LAST retains the preceding snapshot and cursor rather than preparing a new data read.

Both NEW and LAST can return LIST metadata with a blank usage. Tape INPUT/READ to either marker returns the reference's `0xff` sentinel. Rust PIA tests cover marker metadata, retained state, NEW conversion, and LAST write rejection; this is not a claim of broader physical-tape compatibility.

= Further reading

The repository's `README.md` links to the 4050 BASIC reference and describes the emulator's device configuration. The supplied `dev/src/4051-Emulator/web/4051-Emulator/prog.txt` and SimpleDemo programs illustrate additional statements and graphics. The project #link("glossary.typ")[glossary] defines terms including BASIC, GPIB, PIA, and ROM.