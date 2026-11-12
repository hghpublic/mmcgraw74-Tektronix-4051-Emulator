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
  #text(size: 15pt)[SimpleDemo FlashDrive Analysis and Test Design]
]

#align(center)[
  Reference programs: `dev/src/4051-Emulator/web/FlashDrive/SimpleDemo`
  \
  Emulator: `dev/src/4051-Emulator/web/4051-Emulator`
  \
  Automation: `dev/src/4051-Emulator/web/playwright/FlashDrive/SimpleDemo`
  \
  Date: 12 November 2026
]

#outline(title: [Contents], depth: 2)

= Purpose and boundaries

This document records the contents of the `SimpleDemo` FlashDrive directory
and the browser-automation strategy for running its programs. The test
scenarios and outputs stay under `web/playwright/FlashDrive/SimpleDemo`. Dependencies are
shared through the repository-root npm workspace, `node_modules`, and lockfile.

The emulator at `web/4051-Emulator` and all files in `web/FlashDrive` are
read-only inputs. Tests import the original files into the emulator's
in-memory tape storage; they do not rewrite, rename, or export over those
files. A new browser page is used for each test so program 3's in-memory
changes cannot affect another test.

= Directory inventory

#table(
  columns: (0.65fr, 0.95fr, 1.15fr, 2.4fr),
  stroke: 0.5pt,
  [*Number*], [*Type*], [*Name*], [*Role and observed behavior*],
  [1], [ASCII PROG], [Simple Demo], [Displays the pictures in files 23 and 9. It accepts keyboard input and then loads program 2.],
  [2], [ASCII PROG], [Instant Art], [Prompts for text, draws a generated vector pattern, and returns to its prompt. `Q` requests program 1.],
  [3], [ASCII PROG], [Simple create], [Reads file 9 and uses the `CHANGE` and `RDRAW` vector routines while creating output in tape storage.],
  [9], [BINARY DATA], [Death Star], [Picture data consumed by the SimpleDemo programs.],
  [23], [BINARY DATA], [Mickey Mouse], [Picture data consumed by the SimpleDemo programs.],
  [119], [ASCII PROG], [TLIST], [Lists tape directory entries and waits for RETURN before loading program 1.],
  [124], [LAST], [Directory terminator], [FlashDrive end marker; imported as metadata, not executed as a BASIC program.],
)

The filenames encode the tape file number, type, usage, description, and
length. The emulator's FlashDrive importer parses this metadata, so the test
selects the original files by path and preserves their names.

= Emulator integration

The HTML page registers the tape drive at GPIB address 5. Its Storage dialog
provides a `4050 Flash Drive files` import mode that accepts multiple files.
The test imports the directory's files through that UI and verifies the
resulting file-number list is `1, 2, 3, 9, 23, 119, 124`.

The emulator's Auto Load helper sends `FIND @5:1`, `OLD @5:`, and `RUN`; it is
specific to file 1. To run the other programs, the test sends the equivalent
commands with each target number using the emulator's keyboard event path.
This tests the browser application and GPIB tape loading without changing its
source. BASIC `FIND` updates the Storage dialog's file-number field, sometimes
with a leading space. Tests instead observe the real storage methods, recording
their operation and numeric file number after each call. The observers preserve
the original method calls and do not read consuming GPIB buffer getters.

= Running the demo interactively

== Opening the emulator

Manual use needs Python 3 and a browser; Node.js and Playwright are needed only
for automation. From the repository root, start a local server:

```sh
python3 -m http.server 8876 --bind 127.0.0.1 \
  --directory dev/src/4051-Emulator/web/4051-Emulator
```

Keep that terminal open and visit `http://127.0.0.1:8876/Tektronix4051.html`.
If the automation server is already serving that address, use the existing
page rather than starting a second server on the same port.

Click *start* near the bottom of the emulator. Wait for the startup cursor
and for the BUSY lamp to become dark green. The enabled Storage button alone
does not mean ROM startup is complete. Keep the speed control at *x1* while
typing; click its displayed multiplier to cycle speeds when necessary.

== Importing the SimpleDemo tape

+ Click *storage*, select *4050 Flash Drive files*, and click *Import*.
+ In the host file chooser, open
  `dev/src/4051-Emulator/web/FlashDrive/SimpleDemo` and select all seven files.
  Keep the original filenames, including their spaces and leading numbers.
+ Accept the overwrite confirmation for matching file numbers in this
  browser's tape store. Wait for *Done.* and verify entries
  `1, 2, 3, 9, 23, 119, 124` appear.
+ Close the Storage dialog with *Cancel*. This closes the dialog without
  undoing the import. Click the emulator screen before typing.

The tape is in browser memory, not a mounted host directory. Import does not
change the supplied files on disk. Reloading the browser page loses the
imported tape and any in-memory edits; import again after a reload.

== Loading and playing Simple Demo

Type these BASIC commands separately, pressing RETURN after each line:

```text
FIND @5:1
OLD @5:
RUN
```

After FIND and OLD, wait for the command to finish and BUSY to go dark before
typing the next line. Check that the entire FIND command, not just `@5:1`,
appears on screen. Type normally rather than pasting a multiline block.
Host keyboard mappings can differ; check the echoed punctuation and use the
on-screen @ and colon keys if needed.

The running program behaves as follows:

+ Mickey Mouse is drawn from file 23. When the I/O lamp is bright green,
  enter `N` and RETURN to continue.
+ Death Star is drawn from file 9. Again wait for bright I/O, then enter `N`
  and RETURN.
+ Instant Art loads from file 2. Wait for its text prompt and bright I/O;
  enter a name or other text, for example `TEK4051`, then RETURN.
+ Watch the generated pattern. When the text prompt returns, enter different
  text for another pattern, or `Q` and RETURN to return to Mickey Mouse.

The picture program may show only the picture name while waiting for input.
During BASIC INPUT, BUSY can stay bright: the bright *I/O* lamp is the signal
to answer. Do not send the next answer while a picture or pattern is still
being drawn. `Q` at either picture reloads program 1; it does not quit the
emulator.

== Other programs and session controls

To select another program, first interrupt the running BASIC program with
the on-screen *BREAK* key and wait for immediate-command readiness. Repeat
the three-command sequence above, changing only the number after `FIND @5:`.

#table(
  columns: (0.65fr, 1.15fr, 3fr),
  stroke: 0.5pt,
  [*Number*], [*Program*], [*Interactive use*],
  [2], [Instant Art], [Enter text at the prompt; Q returns to Simple Demo.],
  [119], [TLIST], [Lists through LAST entry 124. RETURN loads Simple Demo, even though its prompt calls this a root main menu.],
  [3], [Simple create], [Rewrites picture file 9 in browser memory. Reload the page and reimport the original tape to restore a clean reference session.],
)

*pause* suspends emulation; *start* resumes it. *reset* restarts the emulated
CPU and display, rather than acting as a program-selection menu. To finish,
close the browser tab and stop a server you started with Ctrl+C in its
terminal. Leave an existing server owned by another session alone.

== Avoiding input errors

Message 59 (*no program found*) after OLD can mean FIND was lost or the tape
was not imported. Message 36 (*undefined variable in immediate line*) can
mean an intended program answer was entered when BASIC was not waiting for
it. Do not keep sending answers after an error: return to command readiness,
check the import, and repeat FIND, OLD, and RUN at x1, waiting between them.

The Playwright command `npm run test:ui` opens a scripted test runner, not a
free-play session. Use the manually served emulator above for your own
inputs. Automated tests exercise the emulator's keyboard event path; they
do not certify every physical host-key mapping.

= Automated scenarios

== Importing the directory

Open the emulator, start it, import all seven FlashDrive entries, and verify
that each file number appears in the Storage list in numeric order. The
directory terminator is imported but is not run.

== Running Simple Demo and Instant Art

Run program 1 and verify reads from file 23 and its input-ready I/O indicator.
Capture Mickey Mouse, send a fixed non-quit answer, and verify a read from
file 9 and the next input-ready indicator. Capture Death Star, send another
answer, and verify that program 2 is loaded. Send fixed text to Instant Art
and check for artwork below the text prompt.

== Running Instant Art directly

Load and run program 2 independently, send fixed text, and check for visible
canvas output. The smoke check does not compare the artwork pixel-for-pixel.

== Running Simple create

Load and run program 3 with file 9 present. Verify a read from file 9,
completion, a changed nonempty in-memory data file, and visible canvas output.
The test runs in a fresh page to isolate that rewrite.

== Running TLIST

Load and run program 119, verify traversal reaches file 124, and capture the
directory listing. Send RETURN only when the program is input-ready, then
verify that program 1 loads and reads picture file 23.

= Acceptance and limitations

Every scenario must import the original files, issue the intended BASIC
load/run commands, verify its expected tape operations and output, and complete
without an uncaught page error or unexpected browser dialog. The suite uses
fixed input but does not assert exact display pixels or perform generic OCR.

The original automation typed before ROM startup completed, dropping `FIND`
and leaving `@5:1`. `OLD` then had no prepared program buffer, causing message
59; later answers were interpreted as immediate BASIC, causing message 36.
The runner now waits for the ROM-initialized idle BUSY lamp, sends each command
separately, and verifies FIND and OLD tape operations before continuing.
Program answers wait for the I/O input indicator rather than fixed delays:
BUSY remains illuminated during BASIC INPUT. A 50 ms key-up interval allows
the periodic keyboard scan to detect releases, including repeated digits.
Five strengthened scenarios passed after these changes; the demo reached
Instant Art and generated artwork with no observed messages 59 or 36.

The TLIST trace later showed three temporary failed `expect.poll` attempts
before its file-23 read succeeded; the enclosing test completed and displayed
Mickey Mouse. Tape-operation checks now use a named browser-function wait
with a 30-second timeout, so normal waiting is not shown as failed assertions
in Playwright UI. File 23 is read as picture data, not loaded with OLD.
The ROM-startup check uses the same named-wait approach, preserving its
30-second timeout without reporting temporary assertion failures while the
BUSY lamp is still uninitialized.

For visual inspection, the browser viewport is 1600 by 1200 CSS pixels with
2x pixel density, and each test saves a 3200 by 2400 PNG screenshot. The
emulator canvas remains 1046 by 830 pixels; this change avoids viewport
downscaling but adds no emulated drawing detail.

The test runner requires Node.js, Python 3 for the local static server, the
Playwright package, and its Chromium browser. Docker is not required. The
upstream emulator package's `npm test` script is a placeholder, so the
example package supplies its own `npm test` command rather than changing the
upstream package manifest. From the repository root, `npm ci` installs shared
test dependencies, `npm test` runs all example packages, and
`npm run test:ui` opens SimpleDemo's interactive runner. New example packages
under `web/playwright/FlashDrive` share the root installation rather than installing their
own copies. The root `.npmrc` keeps npm's cache in `.npm-cache` for commands
run from the repository root, without changing user or global settings.
