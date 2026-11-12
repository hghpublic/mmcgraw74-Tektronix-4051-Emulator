import { fileURLToPath } from "node:url";
import { emulatorConfig } from "../../support/config.mjs";

export default emulatorConfig(fileURLToPath(new URL(".", import.meta.url)));
