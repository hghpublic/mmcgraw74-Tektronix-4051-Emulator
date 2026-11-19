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
  #text(size: 15pt)[Glossary]
]

#align(center)[Updated: 15 November 2026]

#outline(title: [Contents], depth: 2)

= Glossary

== BASIC

*Beginner's All-purpose Symbolic Instruction Code.* A programming language used to enter and run programs on the Tektronix 4051. The emulator runs the 4051 BASIC firmware and supports some tested program workflows; full BASIC compatibility remains unverified.

== GPIB

*General Purpose Interface Bus.* A standardized parallel interface, also known as IEEE 488, used to communicate with addressed instruments and peripherals. In the emulator, GPIB connects the machine to devices such as the program loader, real-time clock, and tape drive.

== Host runtime

*Host-side timing controller.* In the Rust port, `HostRuntime` advances emulation using elapsed time supplied by the native application or tests. It schedules CPU execution and keyboard callbacks every 10 ms, phosphor refresh every 100 ms, and independent erase completions after 500 ms. Pausing stops the interval callbacks but allows erase timeouts to advance. It is part of the emulator's host software, not the 4051 BASIC firmware.

== Machine bus

*Emulated hardware access layer.* In the Rust port, `MachineBus` routes processor memory reads and writes to RAM, ROM, and memory-mapped input/output registers. PIA accesses also drive keyboard, display, and GPIB behavior. This is the machine's internal access layer, not the external GPIB interface.

== PIA

*Peripheral Interface Adapter.* An interface between the processor and external signals, with data and control registers for input, output, and interrupt signaling. The 4051 uses PIAs for functions including keyboard, display, and GPIB communication.

== RTC

*Real-Time Clock.* The GPIB device at address 2 that returns the host-local date and time when requested. Its response uses English month abbreviations and millisecond precision; the year is modulo 100 and is not zero-padded.

== ROM

*Read-only memory.* Memory that retains its contents without power and is not normally written through the processor's ordinary memory interface. The 4051's base ROM contains BASIC firmware; additional ROMs provide optional functionality.

== Tape

*Program and data storage device.* The emulator models a 4924 tape drive at GPIB address 5 using numbered files held in memory. `FIND` selects a file; `SAVE` and `OLD` save and load programs, while `PRINT`/`INPUT` transfer ASCII data and `WRITE`/`READ` transfer binary data. The Rust tape implementation remains partial: native file import, Auto Load, and some tape commands are not yet available.