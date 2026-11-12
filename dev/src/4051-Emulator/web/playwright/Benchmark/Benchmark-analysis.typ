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
  #text(size: 15pt)[Benchmark Analysis and Autoplay Design]
]

#align(center)[
  Reference assets: `web/Benchmark`
  \
  Automation: `web/playwright/Benchmark`
  \
  Date: 12 November 2026
]

#outline(title: [Contents], depth: 2)

= Purpose and boundaries

This document analyzes the supplied Benchmark tape files, their execution
paths, the Playwright autoplay coverage, and the known compatibility limits.
Paths beginning with `web/` are relative to `dev/src/4051-Emulator`.

The emulator in `web/4051-Emulator`, the Benchmark assets, and the FlashDrive
assets are read-only references. Autoplay imports the original filenames
through the Storage dialog into a fresh browser page for each test. Program
writes affect only the browser-memory tape, not the source files.

The selected policy is *unchanged-reference behavior*: do not repair BASIC
lines in memory, create alternate tape-file aliases, or substitute a fake
clock. A passing test may verify a documented reference limitation; it must
not be described as proof that the affected program completes normally.

= Asset inventory

#table(
  columns: (0.55fr, 1fr, 1.2fr, 2.45fr),
  stroke: 0.5pt,
  [*File*], [*Type*], [*Description*], [*Role*],
  [1], [ASCII PROG], [Benchmark], [Machine identification and benchmark selector.],
  [2], [ASCII PROG], [Unnamed], [Emulator speed sweep using the GPIB clock.],
  [3], [ASCII PROG], [4050 Bench], [Calculation test with manual stopwatch prompts.],
  [4], [ASCII PROG], [MFM Bench], [Clock-based benchmark requiring the MFM `!TIME` interface.],
  [10], [ASCII DATA], [EM bench Results], [Supplied historical timing records; overwritten only in browser memory by a new sweep.],
  [11], [ASCII PROG], [TLIST], [Directory listing and return to the benchmark selector.],
  [12], [LAST], [Terminator], [End-of-directory metadata, not an executable program.],
)

The filenames encode tape number, type, usage, description, and length. The
importer must receive the original names so the programs retain their intended
file numbers. The expected imported list is `1, 2, 3, 4, 10, 11, 12`.

= Program workflows

== Selector and calculation test

Program 1 compares model-specific DATA values with `RND(0)`. On the 4051 path,
it asks whether the machine is the emulator. Uppercase `Y` loads program 2;
other answers take the program-3 path. For other recognized model paths, it
asks about the MFM clock ROM and can select program 4. These later-model paths
are source analysis, not exercised 4051 autoplay coverage.

The comparisons are case-sensitive. The runner explicitly holds Shift while
entering `Y` or `N`, releases it, and then sends RETURN. The two menu tests
verify the resulting actual `gpibOld` operation on file 2 or 3.

Program 3 waits for RETURN, executes its arithmetic loop, prints DONE and X,
and returns to idle. Autoplay runs this path accelerated; it is not a manual
stopwatch measurement. The test checks input readiness, added canvas activity,
and idle, then saves a screenshot. It does not inspect the numeric variable X.

Although the programs describe 1000 calculations of `X=X*1.01`, their loops
also use X as the FOR counter. Preserve that source behavior; do not interpret
the benchmark as a separately counted 1000-step multiplication recurrence.

== Seven-speed emulator sweep

Program 2 selects tape file 10 at BASIC line 170. Its speed DATA sequence is
`1, 2, 5, 10, 20, 50, 100, 0`, with zero ending the sweep. The displayed
instruction asks the operator to change emulator speed; the BASIC program
does not change the browser's CPU-speed control itself.

At each input prompt, autoplay selects the corresponding actual CPU speed
before sending RETURN and leaves it unchanged until that timing row has been
written. The program obtains timestamps through `INPUT @2:A$` and
`INPUT @2:C$`, converts their time fields to seconds, and subtracts them.
It writes one formatted timing row per speed and finally `|EOT|`.

The runner waits for successive newly written row counts in the browser-memory
file 10, rather than accepting the seven historical rows imported initially.
After the sweep it checks all seven row formats, their speed order, and EOT,
and attaches the new file content as `benchmark-results.txt`.

== File-51 readback limitation

The original program's output and readback targets differ:

```basic
170 FIND @5:10
470 PRINT @5: USING "3D.2D,30A,4D,1A":T,S$,D,"X"
520 FIND @5:51
530 INPUT @5:A$
```

File 51 is absent from the supplied directory. The recorded browser run
produced all seven records in file 10, then repeatedly called `gpibInput`
on file 51. BUSY remained lit and the expected printed results table did
not appear. This is an observed readback loop, not an observed BASIC error.

The sweep test carries a `known-limitation` annotation. It verifies at least
two file-51 input operations and the lit BUSY indicator instead of waiting for
normal termination. Passing means that measurement output and the known loop
were reproduced. It does *not* establish that program 2 reaches its END.

== MFM clock dependency

Program 4 obtains time using ROM-extension calls:

```basic
220 CALL "!TIME",D$
230 CALL "!TIME",H,M,S,B
280 CALL "!TIME",H1,M1,S1,B1
300 CALL "!TIME",E$
```

It expects both string and numeric forms of the MFM clock interface. Its string
conversion routine expects a time beginning `HH:MM:SS.mm`. It also contains
retry conditions, including restarting a measurement when elapsed time is
less than 0.7 seconds.

The emulator *does have a clock*: `RTC.js` implements a GPIB device registered
at address 2. It returns a date-and-time string with a colon-separated
millisecond field. Program 2 uses this device through `INPUT @2:`.
That GPIB device does not itself implement `CALL "!TIME"`, and its timestamp
layout is not a drop-in replacement for program 4's parsing.

The autoplay suite treats the MFM interface as unavailable in the current
configuration and explicitly skips program 4. Its source file is imported
by the executed scenarios, but *program 4 has not been executed*. The skip
is a declared compatibility limitation, not a reproduced runtime failure
or evidence that its timing results are correct.

Future support would require either the corresponding ROM/clock behavior
or an explicitly authorized BASIC adaptation. A simple command-name
replacement would not be sufficient: return types, string layout, timing
units, and retry conditions must also be considered.

== Directory listing

Program 11 increments tape numbers, requests directory headers with
`INPUT @5,19:A$`, and skips empty entries until the LAST marker at file 12.
After RETURN, it requests the root directory and loads file 1. Autoplay checks
the actual listing operation on 12, captures the directory, then verifies
the load of program 1 and its input-ready state.

= Autoplay integration

The suite uses `tests/benchmark.spec.mjs`. Import, startup, tape observation,
key entry, speed selection, and screenshots are shared with SimpleDemo through
`web/playwright/support/emulator.mjs`. Browser and static-server settings come
from `web/playwright/support/config.mjs`.

The runner starts the unmodified HTML application and imports all seven
files through the Storage UI. It waits for the BUSY lamp to acquire its
actual idle state before typing, rather than assuming enabled controls
mean ROM initialization has finished.

Loading uses separately synchronized `FIND`, `OLD`, and `RUN` commands.
Input prompts use the I/O lamp; command completion uses idle. A 50 ms
key-release gap allows the emulator keyboard scan to detect repeated keys.
Expected startup, tape, and timing latency is handled with named browser
waits, avoiding temporary failed polling assertions in the report.

Storage observers call the original methods with the original receiver and
arguments, preserve their return values, and record operation/file number.
They do not consume GPIB output-buffer getters. Uncaught browser exceptions
and unexpected dialogs fail each executed test.

The shared root npm dependency installation is used; there is no separate
Benchmark dependency tree. Tests run with Chromium and one worker. The
read-only Python static server binds to `127.0.0.1:8876`; no database or
container service is needed.

= Evidence and interpretation

The last recorded validation on 12 November 2026 completed `npm test` with
exit code 0: Benchmark had *5 passed and 1 skipped*, and SimpleDemo had
*5 passed*. These are earlier runtime results, not a new test execution
performed for this documentation-only update.

#table(
  columns: (0.8fr, 1.5fr, 1.5fr),
  stroke: 0.5pt,
  [*Speed*], [*Supplied file 10 (s)*], [*Recorded autoplay (s)*],
  [1], [13.06], [3.03],
  [2], [5.71], [1.47],
  [5], [2.13], [0.58],
  [10], [1.04], [0.28],
  [20], [0.52], [0.14],
  [50], [0.20], [0.06],
  [100], [0.10], [0.03],
)

The recorded values come from the saved `benchmark-results.txt`, not from
rewriting the supplied result asset. They illustrate host dependence, not
required timings. Neither exact values nor monotonic speedup are test gates.
The source's time-of-day subtraction does not normalize a midnight rollover;
that boundary is not certified by this suite.

Executed tests save `tape-operations.json`, final screenshots, and named stage
screenshots under `test-results/`. The sweep additionally saves its measured
results. The configured viewport is 1600 x 1200 with scale factor 2; verified
PNG dimensions are 3200 x 2400. The emulated canvas remains 1046 x 830, so
the larger screenshots do not increase the emulator's underlying detail.

Generated test evidence and dependency/cache outputs are intentionally
Git-ignored. There is no generic OCR or BASIC-error parser, exact image
comparison, numerical X assertion, MFM execution coverage, or later-model
compatibility certification. In particular, the calculation screenshot and
pixel activity are not a substitute for a numerical correctness test.

= Operation and source references

From the repository root, with Node.js 20+, Python 3, and an already installed
Playwright Chromium:

```sh
npm run test:benchmark
npm run test:benchmark -- --headed
npm run test:benchmark -- --grep "speed sweep"
npm run test:benchmark:ui -- --ui-host=127.0.0.1 --ui-port=9324
npm test
```

Dependencies and the lockfile live at the repository root. `npm ci` restores
them; it does not install Chromium. Do not make account/global changes or
implicitly install a browser when a prerequisite is missing.

Primary analysis references are the original files numbered 1, 2, 3, 4, 10,
11, and 12 in `web/Benchmark`; `RTC.js` and the GPIB registration in
`Tektronix4051.html` under `web/4051-Emulator`; the Benchmark test suite and
README; and the shared support modules. BASIC line numbers above refer to
the numbered statements, not physical text-file lines.

Preserve the current reference policy when using these scenarios as Rust-port
evidence. Any correction of file-51 readback or addition/adaptation of the
MFM clock is a separate authorized change, with its own acceptance criteria.
These browser tests do not freeze the Phase 0 baseline or establish Rust
feature parity.
