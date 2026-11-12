import { expect, test } from "@playwright/test";

const fixture =
  "/dev/src/4051-Emulator/rs/4051-Emulator-rs/platforms/wasm/browser-tests/input.html";

test.beforeEach(async ({ page }) => {
  await page.goto(fixture);
  await page.waitForFunction(() => window.inputHarnessReady === true);
});

test("pairs printable text and releases the same key identity", async ({ page }) => {
  await page.locator("#keyboard-input").focus();
  await page.keyboard.press("a");

  const calls = await page.evaluate(() => window.adapterCalls);
  const down = calls.find(([name]) => name === "key_down");
  const up = calls.find(([name]) => name === "key_up");
  expect(Number.isInteger(down?.[1])).toBe(true);
  expect(down?.slice(0, 1)).toEqual(["key_down"]);
  expect(down?.slice(2)).toEqual([0x41, false, false, false]);
  expect(up).toEqual(["key_up", down[1], false, false]);
  expect(await page.locator("#keyboard-input").inputValue()).toBe("");
  expect(await page.evaluate(() => window.adapterErrors)).toEqual([]);
});

test("keeps every key in a rapid printable text sequence", async ({ page }) => {
  const text = "abcdefghij";
  await page.locator("#keyboard-input").focus();
  await page.keyboard.type(text, { delay: 1 });

  const calls = await page.evaluate(() => window.adapterCalls);
  expect(calls.filter(([name]) => name === "key_down")).toHaveLength(text.length);
  expect(calls.filter(([name]) => name === "key_up")).toHaveLength(text.length);
});

test("uses paired punctuation and event-time Shift inversion", async ({ page }) => {
  await page.locator("#keyboard-input").focus();
  await page.evaluate(() => {
    const input = document.querySelector("#keyboard-input");
    input.dispatchEvent(new KeyboardEvent("keydown", {
      key: "2",
      code: "Digit2",
      shiftKey: true,
      bubbles: true,
      cancelable: true,
    }));
    const textInput = new InputEvent("beforeinput", {
      data: "@",
      inputType: "insertText",
      bubbles: true,
      cancelable: true,
    });
    input.dispatchEvent(textInput);
    window.pairedTextDefaultPrevented = textInput.defaultPrevented;
    input.dispatchEvent(new KeyboardEvent("keyup", {
      key: "2",
      code: "Digit2",
      shiftKey: true,
      bubbles: true,
      cancelable: true,
    }));
  });

  const down = await page.evaluate(
    () => window.adapterCalls.find(([name]) => name === "key_down"),
  );
  expect(Number.isInteger(down?.[1])).toBe(true);
  expect(down?.slice(2)).toEqual([0x40, true, true, false]);
  expect(await page.evaluate(() => window.pairedTextDefaultPrevented)).toBe(true);
});

test("waits for delayed layout text before mapping a printable key", async ({ page }) => {
  await page.locator("#keyboard-input").focus();
  await page.evaluate(() => {
    document.querySelector("#keyboard-input").dispatchEvent(new KeyboardEvent("keydown", {
      key: "+",
      code: "Equal",
      shiftKey: true,
      bubbles: true,
      cancelable: true,
    }));
  });
  await page.waitForTimeout(10);
  await page.evaluate(() => {
    const input = document.querySelector("#keyboard-input");
    const textInput = new InputEvent("beforeinput", {
      data: "*",
      inputType: "insertText",
      bubbles: true,
      cancelable: true,
    });
    input.dispatchEvent(textInput);
    input.dispatchEvent(new KeyboardEvent("keyup", {
      key: "+",
      code: "Equal",
      shiftKey: true,
      bubbles: true,
      cancelable: true,
    }));
  });

  const calls = await page.evaluate(() => window.adapterCalls);
  const down = calls.find(([name]) => name === "key_down");
  expect(Number.isInteger(down?.[1])).toBe(true);
  expect(down?.slice(2)).toEqual([0x2a, false, true, false]);
  expect(calls.filter(([name]) => name === "key_up")).toEqual([
    ["key_up", down?.[1], true, false],
  ]);
});

test("passes each printable key's event-time Shift state with paired text", async ({ page }) => {
  await page.locator("#keyboard-input").focus();
  await page.evaluate(() => {
    const input = document.querySelector("#keyboard-input");
    for (const [key, shiftKey] of [
      ["+", false],
      ["=", true],
      ["<", false],
      [";", true],
    ]) {
      input.dispatchEvent(new KeyboardEvent("keydown", {
        key,
        shiftKey,
        bubbles: true,
        cancelable: true,
      }));
      input.dispatchEvent(new InputEvent("beforeinput", {
        data: key,
        inputType: "insertText",
        bubbles: true,
        cancelable: true,
      }));
      input.dispatchEvent(new KeyboardEvent("keyup", {
        key,
        shiftKey,
        bubbles: true,
        cancelable: true,
      }));
    }
  });

  const mappings = await page.evaluate(
    () => window.browserMappingCalls
      .filter(([, text]) => typeof text === "string")
      .map(([key, text, shift]) => [key, text, shift]),
  );
  expect(mappings).toEqual([
    ["+", "+", false],
    ["=", "=", true],
    ["<", "<", false],
    [";", ";", true],
  ]);
});

test("falls back to the logical key on key-up when no text event arrives", async ({ page }) => {
  await page.locator("#keyboard-input").focus();
  await page.evaluate(() => {
    const input = document.querySelector("#keyboard-input");
    for (const type of ["keydown", "keyup"]) {
      input.dispatchEvent(new KeyboardEvent(type, {
        key: "a",
        code: "KeyA",
        bubbles: true,
        cancelable: true,
      }));
    }
  });

  const calls = await page.evaluate(() => window.adapterCalls);
  const down = calls.find(([name]) => name === "key_down");
  expect(Number.isInteger(down?.[1])).toBe(true);
  expect(down?.slice(2)).toEqual([0x41, false, false, false]);
  expect(calls.filter(([name]) => name === "key_up")).toEqual([
    ["key_up", down?.[1], false, false],
  ]);
});

test("maps function keys and sends modifier-only changes", async ({ page }) => {
  await page.locator("#keyboard-input").focus();
  await page.keyboard.press("F1");
  await page.evaluate(() => {
    const input = document.querySelector("#keyboard-input");
    input.dispatchEvent(new KeyboardEvent("keydown", {
      key: "Shift",
      code: "ShiftLeft",
      shiftKey: true,
      bubbles: true,
    }));
    input.dispatchEvent(new KeyboardEvent("keyup", {
      key: "Shift",
      code: "ShiftLeft",
      bubbles: true,
    }));
    input.dispatchEvent(new KeyboardEvent("keydown", {
      key: "a",
      code: "KeyA",
      repeat: true,
      bubbles: true,
    }));
  });

  const calls = await page.evaluate(() => window.adapterCalls);
  const down = calls.find(([name]) => name === "key_down");
  expect(Number.isInteger(down?.[1])).toBe(true);
  expect(down?.slice(2)).toEqual([0x60, false, false, false]);
  expect(calls.filter(([name]) => name === "set_modifiers")).toEqual([
    ["set_modifiers", true, false],
    ["set_modifiers", false, false],
  ]);
  expect(calls.filter(([name]) => name === "key_down")).toHaveLength(1);
});

test("does not claim unpaired Alt shortcuts or keys from text controls", async ({ page }) => {
  await page.locator("#keyboard-input").focus();
  await page.evaluate(() => {
    const input = document.querySelector("#keyboard-input");
    const metaShortcut = new KeyboardEvent("keydown", {
      key: "a",
      code: "KeyA",
      metaKey: true,
      bubbles: true,
      cancelable: true,
    });
    input.dispatchEvent(metaShortcut);
    window.metaShortcutPrevented = metaShortcut.defaultPrevented;
    const altShortcut = new KeyboardEvent("keydown", {
      key: "a",
      code: "KeyA",
      altKey: true,
      bubbles: true,
      cancelable: true,
    });
    input.dispatchEvent(altShortcut);
    window.altShortcutPrevented = altShortcut.defaultPrevented;
  });
  await page.waitForTimeout(10);
  await page.locator("#other-input").focus();
  await page.keyboard.press("a");

  const calls = await page.evaluate(() => window.adapterCalls);
  expect(calls.filter(([name]) => name === "key_down")).toEqual([]);
  expect(await page.evaluate(() => window.altShortcutPrevented)).toBe(false);
  expect(await page.evaluate(() => window.metaShortcutPrevented)).toBe(false);
  expect(await page.locator("#other-input").inputValue()).toBe("a");
});

test("releases held keys on focus loss", async ({ page }) => {
  await page.locator("#keyboard-input").focus();
  await page.keyboard.down("Backspace");
  await page.evaluate(() => window.dispatchEvent(new Event("blur")));

  const calls = await page.evaluate(() => window.adapterCalls);
  expect(calls.filter(([name]) => name === "key_down")).toHaveLength(1);
  expect(calls.filter(([name]) => name === "release_input")).toHaveLength(1);
  await page.keyboard.up("Backspace");
  expect(await page.evaluate(
    () => window.adapterCalls.filter(([name]) => name === "key_up").length,
  )).toBe(0);
});

test("releases held keys when the document becomes hidden", async ({ page }) => {
  await page.locator("#keyboard-input").focus();
  await page.keyboard.down("Backspace");
  await page.evaluate(() => {
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      value: "hidden",
    });
    document.dispatchEvent(new Event("visibilitychange"));
  });

  const calls = await page.evaluate(() => window.adapterCalls);
  expect(calls.filter(([name]) => name === "release_input")).toHaveLength(1);
});

test("pairs pointer panel press and release with captured pointer identity", async ({ page }) => {
  const button = page.locator("#panel-key");
  await button.hover();
  await page.mouse.down();
  await page.mouse.up();

  const calls = await page.evaluate(() => window.adapterCalls);
  expect(calls.filter(([name]) => name === "panel_press")).toEqual([
    ["panel_press", 0x61, false, false],
  ]);
  expect(calls.filter(([name]) => name === "panel_release")).toEqual([
    ["panel_release", false, false],
  ]);
  expect(await page.evaluate(() => window.adapterErrors)).toEqual([]);
});

test("uses fixed Shift overrides for panel buttons", async ({ page }) => {
  await page.keyboard.down("Shift");
  for (const selector of ["#panel-key-shift", "#panel-key-normal"]) {
    const button = page.locator(selector);
    await button.hover();
    await page.mouse.down();
    await page.mouse.up();
  }
  await page.keyboard.up("Shift");

  const calls = await page.evaluate(() => window.adapterCalls);
  expect(calls.filter(([name]) => name === "panel_press")).toEqual([
    ["panel_press", 0x60, true, false],
    ["panel_press", 0x60, false, false],
  ]);
  expect(calls.filter(([name]) => name === "panel_release")).toEqual([
    ["panel_release", true, false],
    ["panel_release", false, false],
  ]);
  expect(await page.evaluate(() => window.adapterErrors)).toEqual([]);
});

test("releases a panel press on pointer cancellation and lost capture", async ({ page }) => {
  const button = page.locator("#panel-key");
  await button.hover();
  await page.mouse.down();
  const firstPointerId = await page.evaluate(() => window.panelPointerId);
  await button.dispatchEvent("pointercancel", { pointerId: firstPointerId, shiftKey: true });
  await page.mouse.up();

  await button.hover();
  await page.mouse.down();
  const secondPointerId = await page.evaluate(() => window.panelPointerId);
  await page.evaluate((pointerId) => {
    document.querySelector("#panel-key").dispatchEvent(new PointerEvent("lostpointercapture", {
      bubbles: true,
      pointerId,
    }));
  }, secondPointerId);
  await page.mouse.up();

  const calls = await page.evaluate(() => window.adapterCalls);
  expect(calls.filter(([name]) => name === "panel_press")).toHaveLength(2);
  expect(calls.filter(([name]) => name === "panel_release")).toEqual([
    ["panel_release", true, false],
    ["panel_release", false, false],
  ]);
  expect(await page.evaluate(() => window.adapterErrors)).toEqual([]);
});

test("allows only one panel key owner per emulator session", async ({ page }) => {
  await page.locator("#panel-key").hover();
  await page.mouse.down();
  await page.locator("#panel-key-two").dispatchEvent("pointerdown", {
    pointerId: 99,
    isPrimary: true,
    button: 0,
    bubbles: true,
    cancelable: true,
  });
  await page.mouse.up();

  const calls = await page.evaluate(() => window.adapterCalls);
  expect(calls.filter(([name]) => name === "panel_press")).toEqual([
    ["panel_press", 0x61, false, false],
  ]);
  expect(calls.filter(([name]) => name === "panel_release")).toEqual([
    ["panel_release", false, false],
  ]);
});
