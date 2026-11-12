import { expect, test } from "@playwright/test";
import {
  answerProgram,
  attachScreen,
  expectTapeOperation,
  runProgram,
  waitForIdle,
  waitForProgramInput,
} from "../../../support/emulator.mjs";
import { useStarWarsTape } from "./tape.mjs";

useStarWarsTape();

test("TLIST lists through LAST and RETURN runs the original file 1 generator", async ({ page }) => {
  await runProgram(page, 119);
  await expectTapeOperation(page, "gpibList", 124);
  await waitForProgramInput(page);
  expect(await page.evaluate(
    () => window.tapeEvents.filter((event) => event.operation === "gpibList").map((event) => event.fileNumber),
  )).toEqual(Array.from({ length: 124 }, (_, index) => index + 1));
  await attachScreen(page, "StarWars directory");

  await answerProgram(page, "");
  await expectTapeOperation(page, "gpibOld", 1);
  await expectTapeOperation(page, "gpibFind", 6);
  await waitForIdle(page);
  expect(await page.evaluate(() => storage.fstore.getFile(6).content.byteLength)).toBe(3990);
  await attachScreen(page, "TLIST return runs TIE generator");
});
