import { expect, test } from "@playwright/test";
import {
  answerProgram,
  attachScreen,
  expectTapeOperation,
  runProgram,
  sendTekText,
  setCpuSpeed,
  waitForIdle,
  waitForProgramInput,
} from "../../../support/emulator.mjs";
import { useStarWarsTape } from "./tape.mjs";

useStarWarsTape();

for (const { selection, file, label } of [
  { selection: "1", file: 6, label: "ROTJ" },
  { selection: "2", file: 4, label: "new-design" },
]) {
  test(`R12 game loads the ${label} fighter and accepts rank zero to end`, async ({ page }) => {
    await runProgram(page, 2);
    await answerProgram(page, selection);
    await expectTapeOperation(page, "gpibRead", file);
    await waitForProgramInput(page);
    await attachScreen(page, `${label} instructions and ranks`);
    await answerProgram(page, "0");
    await waitForIdle(page);
    expect(await page.locator("#status_io").evaluate((element) => getComputedStyle(element).color))
      .not.toBe("rgb(0, 238, 0)");
    await attachScreen(page, `${label} turned in wings`);
  });
}

test("R12 game draws the cockpit and UDK16 recalls instructions before ending", async ({ page }) => {
  await runProgram(page, 2);
  await answerProgram(page, "1");
  await expectTapeOperation(page, "gpibRead", 6);
  await waitForProgramInput(page);
  const instructions = await instructionHeader(page);
  await answerProgram(page, "1");
  await answerProgram(page, "Y");
  await test.step("Waiting for the completed cockpit outline after PAGE", async () => {
    await page.waitForFunction(() => {
      const canvas = document.getElementById("screen");
      const pixels = canvas.getContext("2d").getImageData(0, 300, canvas.width, canvas.height - 300).data;
      let lit = 0;
      let leftEdge = 0;
      for (let index = 0; index < pixels.length; index += 4) {
        if (pixels[index + 1] > 0) {
          lit += 1;
          if ((index / 4) % canvas.width < canvas.width / 20) leftEdge += 1;
        }
      }
      return lit > 2_000 && lit < canvas.width * (canvas.height - 300) / 5
        && leftEdge > 300
        && getComputedStyle(document.getElementById("status_busy")).color === "rgb(0, 238, 0)"
        && getComputedStyle(document.getElementById("status_io")).color !== "rgb(0, 238, 0)";
    });
  });
  await setCpuSpeed(page, 1);
  await attachScreen(page, "R12 cockpit and TIE fighter");
  await page.keyboard.down("Shift");
  try {
    await sendTekText(page, String.fromCharCode(0x65));
  } finally {
    await page.keyboard.up("Shift");
  }
  await setCpuSpeed(page, 100);
  await waitForProgramInput(page);
  expect(await instructionHeader(page), "UDK16 restored the instruction heading, not the score prompt")
    .toEqual(instructions);
  await attachScreen(page, "UDK16 recalled instructions");
  await answerProgram(page, "0");
  await waitForIdle(page);
});

test.skip("4054 game (file 3) requires Option 30 RINIT, ROPEN and VISIBILITY", async () => {});

async function instructionHeader(page) {
  return page.locator("#screen").evaluate(async (canvas) => {
    // Exclude the blinking input cursor below the instruction heading.
    const pixels = canvas.getContext("2d").getImageData(0, 0, canvas.width, 150).data;
    return Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", pixels)));
  });
}
