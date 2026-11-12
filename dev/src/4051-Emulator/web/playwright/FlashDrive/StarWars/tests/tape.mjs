import { fileURLToPath } from "node:url";
import { useTapePrograms } from "../../../support/emulator.mjs";

export const directory = fileURLToPath(new URL("../../../../FlashDrive/StarWars/", import.meta.url));
export const expectedFileNumbers = ["1", "2", "3", "4", "5", "6", "12", "13", "14", "119", "124"];

export function useStarWarsTape() {
  useTapePrograms(directory, expectedFileNumbers);
}
