#set par(justify: true)
#set page(
  margin: 40mm,
  header-ascent: 18pt,
  footer-descent: 18pt,
  header: context {
    if counter(page).get().first() > 1 {
      let prior = query(heading.where(level: 1, outlined: true))
        .filter(heading => heading.location().page() <= here().page())
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
    let page_number = counter(page).display("1")
    if counter(page).get().first() == 1 {
      align(center)[#page_number]
    } else {
      align(right)[#page_number]
    }
  },
)
#set text(font: "Arial", size: 10pt)
#set heading(numbering: "1.1")

#align(center)[
  #text(size: 20pt, weight: "bold")[Direct-View Bistable Storage Tube]
]

#outline(title: [Contents], depth: 2)

The #link("https://w140.com/tekwiki/images/b/b9/Tek_4051_basicref.pdf")[Tektronix 4051] (introduced in 1975) uses an 11-inch #link("https://en.wikipedia.org/wiki/Direct-view_bistable_storage_tube")[Direct-View Bistable Storage Tube] (DVST), a specialized vector cathode-ray tube (CRT) engineered by Tektronix. Unlike standard raster monitors that require expensive video RAM to continuously refresh an image, this tube physically stores the drawn graphics directly on its phosphor screen. 

[1] #link("https://en.wikipedia.org/wiki/Tektronix_4050")[Tektronix 4050], \

= How the Storage Tube Works

The DVST operates using two distinct electron guns rather than the single gun found in conventional CRTs: (#link("https://t-lcarchive.org/tektronix-4051/")[Tektronix 4051])

- Writing Gun. A high-velocity electron beam draws sharp, continuous vector lines and characters onto a storage dielectric layer right behind the screen. (#link("https://www.youtube.com/watch?v=T4YYlATuiTY")[The Tektronix 4051 - Wonder Computer of the Mid 1970s! BUT CAN IT GAME!? Find out on ARG PRESENTS!])

  [1] #link("https://www.youtube.com/watch?v=T4YYlATuiTY")[The Tektronix 4051 - Wonder Computer of the Mid 1970s! BUT CAN IT GAME!? Find out on ARG PRESENTS!], \
  [2] #link("https://t-lcarchive.org/tektronix-4051/")[Tektronix 4051]

- Flood Gun. A low-velocity electron gun continuously showers the entire screen with electrons. Where the writing gun has altered the electrical charge, electrons pass through and keep the green phosphor glowing indefinitely without flickering.

  [1] #link("https://vintagetek.org/wp-content/uploads/2017/06/Tek-4051-1976.pdf")[Tektronix 4051], \
  [2] #link("https://t-lcarchive.org/tektronix-4051/")[Tektronix 4051],

- Screen Clearing. Because the image is physically latched onto the tube's surface, individual pixels or lines cannot be erased. Clearing the screen requires applying an erase voltage pulse, which causes the entire display to briefly flash bright green before wiping clean.

  [1] #link("https://www.reddit.com/r/vintagecomputing/comments/1kso23s/tektronix_4051_microcomputer_lives_again/")[Tektronix 4051 Microcomputer Lives Again!]

= Key Specifications

* Screen Size & Color. 11-inch diagonal flat-faced screen displaying monochrome green vector graphics.
* Addressable Resolution. 1024 × 780 points, offering remarkably crisp line art for a mid-1970s desktop computer.
* Text Capacity. 35 lines of 72 uppercase and lowercase ASCII characters per page.
* Memory Advantage. Eliminating the need for a frame buffer allowed the system to dedicate all of its internal RAM (8 KB base, expandable to 32 KB) directly to BASIC programs and data. 

[1] #link("https://en.wikipedia.org/wiki/Tektronix_4050")[Tektronix 4050], \
[2] #link("https://vintagetek.org/wp-content/uploads/2017/06/Tek-4051-1976.pdf")[Tektronix 4051], \
[3] #link("https://github.com/mmcgraw74/Tektronix-4051-4052-4054-Program-Files")[Tektronix-4051-4052-4054-Program-Files], \
[4] #link("https://www.youtube.com/watch?v=T4YYlATuiTY")[The Tektronix 4051 - Wonder Computer of the Mid 1970s! BUT CAN IT GAME!? Find out on ARG PRESENTS!], \
[5] #link("https://www.facebook.com/groups/retrocomputers/posts/24325745853695300/")[Retro Microcomputers, Workstations, Servers and Consoles - Welcome!]

= Trade-offs and Quirks

- No Real-Time Animation. Since selective erasure is impossible, the display cannot do traditional scrolling or smooth sprite animation. (However, Tektronix later introduced a "refresh graphics" mode that allowed a small number of non-stored vectors to be drawn continuously without latching, enabling simple cursor movement or basic wireframe previews.) 

  [1] #link("https://www.eevblog.com/forum/chat/tektronix-4051-how-did-it-work/")[Tektronix 4051: How did it work?], \
  [2] #link("https://www.youtube.com/watch?v=xWrIS_l5HWk")[Introduction to the Tektronix 4051: Part 1 (Introduction and Quick Sample Programs)], \
  [3] #link("https://www.youtube.com/watch?v=TDa8Ac8kckQ")[Invader-I on Tektronix 4051 Emulator] \

- Phosphor Burn-In & Aging. Over decades, the flood gun and prolonged static images can degrade the storage target or cause phosphor burn, making intact, bright tubes rare among collectors.

You can explore more technical details and restoration documentation on the #link("https://w140.com/tekwiki/wiki/4051")[TekWiki 4051 Page] or the #link("https://vintagetek.org/4051-graphic-desktop-system/")[vintageTEK Museum Archive].

Are you looking for troubleshooting and repair info for a physical 4051 tube, pinout/service manual schematics, or details on how to program its vector graphics? Let me know what you're working on and I can pull up the exact details.


= Blinking Cursor

The blinking cursor on the Tektronix 4051 works through a special hardware feature called Write-Through Mode. Because a Direct-View Bistable Storage Tube (DVST) permanently latches anything drawn onto its screen until the whole display is erased, drawing a standard blinking cursor would leave a permanent solid block behind. 

[1] (/goto?url=CAESZAHEIkXXW8N4CJKOGzdFvH3GZmBMjzkGReP_BnqmbCwNOtaqJBWCz8uIJv0V9c-irl7AfHqHlZP_arlg3iLtVCzumSQn8_97R9QdELhKGdorWAp1ZdtdhZ6M7IsyVEFeN-IKwzE), [2] (/goto?url=CAESfQHEIkXXAVb-aarlvFR3NNYnVNtwia-FBDkHUDqusc4XSe1vbm-LtsYnT5PEFaYMJCrMB4XivofOP1kfoPSf2sFkfUndANdMTUEnNAOAV_OYqubbYlBaRZ7nkmiX_4alPl8FRLqDRK314MujFWJ_ysNReTqM2gJtLz3ceVyW)

To solve this, Tektronix engineered a way to make the electron beam visible without actually triggering the storage mechanism.

. Lowering Beam Intensity (Write-Through Mode)

To store an image permanently on a DVST, the writing electron gun must hit the storage dielectric layer hard enough to knock loose secondary electrons, shifting the local electrical charge so the flood gun keeps it glowing.

  [1] (/goto?url=CAESZAHEIkXXW8N4CJKOGzdFvH3GZmBMjzkGReP_BnqmbCwNOtaqJBWCz8uIJv0V9c-irl7AfHqHlZP_arlg3iLtVCzumSQn8_97R9QdELhKGdorWAp1ZdtdhZ6M7IsyVEFeN-IKwzE)

- Below the Storage Threshold: In Write-Through Mode, the display circuitry reduces the beam intensity (or shortens the pulse duration) of the writing gun.

  [1] (/goto?url=CAESfQHEIkXXAVb-aarlvFR3NNYnVNtwia-FBDkHUDqusc4XSe1vbm-LtsYnT5PEFaYMJCrMB4XivofOP1kfoPSf2sFkfUndANdMTUEnNAOAV_OYqubbYlBaRZ7nkmiX_4alPl8FRLqDRK314MujFWJ_ysNReTqM2gJtLz3ceVyW)

- Phosphor Excitation Only: The beam still has enough energy to strike the front phosphor layer and produce a brief, faint green glow that the human eye can see.

  [1] (/goto?url=CAESfQHEIkXXAVb-aarlvFR3NNYnVNtwia-FBDkHUDqusc4XSe1vbm-LtsYnT5PEFaYMJCrMB4XivofOP1kfoPSf2sFkfUndANdMTUEnNAOAV_OYqubbYlBaRZ7nkmiX_4alPl8FRLqDRK314MujFWJ_ysNReTqM2gJtLz3ceVyW)

- No Charge Latching: Because the beam energy stays below the secondary-emission threshold, the storage layer behind the glass remains electrically unchanged. As soon as the beam turns off, the glow fades away instantly without leaving a permanent mark.

  [1] (/goto?url=CAESfQHEIkXXAVb-aarlvFR3NNYnVNtwia-FBDkHUDqusc4XSe1vbm-LtsYnT5PEFaYMJCrMB4XivofOP1kfoPSf2sFkfUndANdMTUEnNAOAV_OYqubbYlBaRZ7nkmiX_4alPl8FRLqDRK314MujFWJ_ysNReTqM2gJtLz3ceVyW)

. The Firmware Blink Loop

Because Write-Through graphics fade immediately, the cursor must be continuously redrawn by the 4051's Motorola 6800 processor.

  [1] (/goto?url=CAESkQEBxCJF19W0n1a63vjiR8ZZPPO-St8ls8zxvqoT-GyoDm-206eBU2Pd0IOeF1-3MI9b02N7xp3aeYSsj9iKEl4mzfML-X6HHqzMxOnnr1WK5M8YMpS_FWiN8viX_coQIp7jQRWTb_dPPV8NpGMeW9m7BDqopls97y6kFdlLvuCwj7VKY2vCkl8Edtb3dDvF7tEW)

- Software Delay Loop: When the 4051 is waiting for user input, its internal ROM firmware enters a dedicated cursor routine (DIMSUB-GRAF in the display subsystem).

  [1] (/goto?url=CAESkQEBxCJF19W0n1a63vjiR8ZZPPO-St8ls8zxvqoT-GyoDm-206eBU2Pd0IOeF1-3MI9b02N7xp3aeYSsj9iKEl4mzfML-X6HHqzMxOnnr1WK5M8YMpS_FWiN8viX_coQIp7jQRWTb_dPPV8NpGMeW9m7BDqopls97y6kFdlLvuCwj7VKY2vCkl8Edtb3dDvF7tEW)

- On/Off Toggling: The CPU repeatedly commands the vector generator to draw the cursor shape in Write-Through Mode for a set number of cycles, pauses in a software delay loop while the cursor disappears, and repeats the cycle.

  [1] (/goto?url=CAESkQEBxCJF19W0n1a63vjiR8ZZPPO-St8ls8zxvqoT-GyoDm-206eBU2Pd0IOeF1-3MI9b02N7xp3aeYSsj9iKEl4mzfML-X6HHqzMxOnnr1WK5M8YMpS_FWiN8viX_coQIp7jQRWTb_dPPV8NpGMeW9m7BDqopls97y6kFdlLvuCwj7VKY2vCkl8Edtb3dDvF7tEW)

- Why It Stops During Execution: Because the CPU itself has to actively redraw the non-stored cursor, the cursor vanishes whenever the 4051 is busy running a BASIC program or loading from tape.

  [1] (/goto?url=CAESkQEBxCJF19W0n1a63vjiR8ZZPPO-St8ls8zxvqoT-GyoDm-206eBU2Pd0IOeF1-3MI9b02N7xp3aeYSsj9iKEl4mzfML-X6HHqzMxOnnr1WK5M8YMpS_FWiN8viX_coQIp7jQRWTb_d \
  [2] (/goto?url=CAEShQEBxCJF17BICUJlq8RxYfH65JpQbwrwfYAvcjSrVs0Aj-BgKB4_PPkOtTgiBRwoxb3mvxHpcC0-K8fu0ItbZ2BPQ0fBtu6NCrg-VL-KFsSVfrRDncZXpdjkQ4CZ5U3VGTHeqHTLHOVjQgcXMHW35k1lcAczvRgM9u6RK1izQu6QAuUEjhKp)

. Screen-Full Indicator (Blinking "F")

This same Write-Through mechanism is used when the screen fills up with text. Since a storage tube cannot scroll text upward, reaching the bottom line causes the 4051 to pause and flash a non-stored "F" in the upper-left corner using Write-Through Mode. This alerts the user to press the HOME/PAGE key to wipe the screen clean before continuing.
  
  [1] (/goto?url=CAESdwHEIkXXh31EcsvWCNg2lv1X0lLdLBb99D4NiWtNaM804idwk_qMkyomoZNUFg3jsDAXD49NLcij83E-4U88thpyo6EJhaPJtsmN0bTMEFXQgSlJxHj-905lQsro_1PjYtAd0ed2La6xpFKaZfRfFf-EMhmRK7D1), \
  [2] (/goto?url=CAEScQHEIkXX0Ga6s990LlDxxMiU0g0aH_kSK6GDYBXq28_pmX4jTdOQsz1FZgbbhoDxvs-WF_G7P8-XFv4RXk17rVowsVta_MPX9a5zeUiejaRrjDRNZrpsM-sdF7pTKy2a1nUpMiuH5wF7s_Stwoo77zKx)

You can read more about how the 4050-series handles Write-Through Mode on Electronix & More or explore the ROM cursor routines documented by enthusiasts on the Vintage Computer Federation Forums.

Would you like to look at the display board schematics that control the Write-Through beam intensity, or see how BASIC programs can use Write-Through mode (refresh graphics) for simple animations?
