const INTERACTIVE_TARGETS = [
  "button",
  "input",
  "select",
  "textarea",
  "[contenteditable]:not([contenteditable='false'])",
  "[role='textbox']",
].join(", ");
const activePanelBindings = new WeakMap();

function modifiersFrom(event) {
  return {
    shift: Boolean(event.shiftKey),
    control: Boolean(event.ctrlKey),
  };
}

function isInteractiveTarget(target, keyboardInput) {
  return target !== keyboardInput && Boolean(target?.closest?.(INTERACTIVE_TARGETS));
}

function keyIdentity(event) {
  return event.code && event.code !== "Unidentified" ? event.code : `key:${event.key}`;
}

function requireFunction(value, name) {
  if (typeof value !== "function") {
    throw new TypeError(`${name} must be a function`);
  }
}

export function attachBrowserKeyboard({
  surface,
  keyboardInput,
  emulator,
  mapBrowserKey,
  onError,
}) {
  if (!surface?.addEventListener || !keyboardInput?.addEventListener) {
    throw new TypeError("surface and keyboardInput must be DOM elements");
  }
  requireFunction(mapBrowserKey, "mapBrowserKey");
  requireFunction(onError, "onError");
  if ((typeof emulator !== "object" && typeof emulator !== "function") || emulator === null) {
    throw new TypeError("emulator is required");
  }

  const document = surface.ownerDocument;
  const window = document.defaultView;
  const heldKeys = new Map();
  let nextKeyId = 1;
  let pendingTextKey = null;
  let modifiers = { shift: false, control: false };

  function invoke(method, ...arguments_) {
    try {
      emulator[method](...arguments_);
      return true;
    } catch (error) {
      onError(error);
      return false;
    }
  }

  function mapKey(key, text, shift) {
    try {
      return mapBrowserKey(key, text, shift);
    } catch (error) {
      onError(error);
      return null;
    }
  }

  function updateModifiers(event) {
    const next = modifiersFrom(event);
    if (next.shift !== modifiers.shift || next.control !== modifiers.control) {
      modifiers = next;
      invoke("set_modifiers", next.shift, next.control);
    }
  }

  function clearPendingTextKey() {
    pendingTextKey = null;
  }

  function pressKey(event, mapping) {
    const identity = keyIdentity(event);
    const keyId = nextKeyId;
    nextKeyId = nextKeyId === 0xffff_ffff ? 1 : nextKeyId + 1;
    const currentModifiers = modifiersFrom(event);

    if (invoke(
      "key_down",
      keyId,
      mapping.code,
      mapping.invert_shift,
      currentModifiers.shift,
      currentModifiers.control,
    )) {
      heldKeys.set(identity, { keyId, mapping });
      modifiers = currentModifiers;
    }
  }

  function resolvePendingTextKey(entry, text) {
    if (pendingTextKey !== entry) {
      return;
    }
    clearPendingTextKey();

    if (text === undefined && entry.event.altKey) {
      updateModifiers(entry.event);
      return;
    }

    const mapping = text === undefined
      ? entry.mapping
      : mapKey(entry.event.key, text, entry.event.shiftKey);
    if (!mapping) {
      updateModifiers(entry.event);
      return;
    }
    pressKey(entry.event, mapping);
  }

  function releaseAll() {
    clearPendingTextKey();
    heldKeys.clear();
    modifiers = { shift: false, control: false };
    invoke("release_input");
  }

  function onKeyDown(event) {
    if (isInteractiveTarget(event.target, keyboardInput)) {
      return;
    }

    updateModifiers(event);
    if (event.metaKey || event.isComposing || event.key === "Process") {
      return;
    }

    const identity = keyIdentity(event);
    if (event.repeat) {
      if (heldKeys.has(identity) || pendingTextKey?.identity === identity) {
        event.preventDefault();
      }
      return;
    }
    if (heldKeys.has(identity)) {
      event.preventDefault();
      return;
    }
    if (pendingTextKey) {
      resolvePendingTextKey(pendingTextKey);
    }

    const mapping = mapKey(event.key, undefined, event.shiftKey);
    if (!mapping) {
      return;
    }
    if (event.altKey && !mapping.text_input) {
      return;
    }
    if (!mapping.text_input) {
      pressKey(event, mapping);
      event.preventDefault();
      return;
    }

    const entry = { event, identity, mapping };
    pendingTextKey = entry;
  }

  function onKeyUp(event) {
    if (isInteractiveTarget(event.target, keyboardInput)) {
      return;
    }

    const identity = keyIdentity(event);
    if (pendingTextKey?.identity === identity) {
      resolvePendingTextKey(pendingTextKey);
    }

    const ownedKey = heldKeys.get(identity);
    if (!ownedKey) {
      updateModifiers(event);
      return;
    }

    heldKeys.delete(identity);
    const currentModifiers = modifiersFrom(event);
    modifiers = currentModifiers;
    invoke("key_up", ownedKey.keyId, currentModifiers.shift, currentModifiers.control);
    event.preventDefault();
  }

  function onBeforeInput(event) {
    if (event.target !== keyboardInput || event.inputType !== "insertText") {
      return;
    }
    event.preventDefault();
    if (pendingTextKey) {
      resolvePendingTextKey(
        pendingTextKey,
        typeof event.data === "string" ? event.data : "",
      );
    }
  }

  function onInput() {
    keyboardInput.value = "";
  }

  function onSurfaceClick(event) {
    if (!isInteractiveTarget(event.target, keyboardInput)) {
      keyboardInput.focus({ preventScroll: true });
    }
  }

  function onFocusOut(event) {
    if (!event.relatedTarget || !surface.contains(event.relatedTarget)) {
      releaseAll();
    }
  }

  function onVisibilityChange() {
    if (document.visibilityState === "hidden") {
      releaseAll();
    }
  }

  surface.addEventListener("keydown", onKeyDown);
  surface.addEventListener("keyup", onKeyUp);
  surface.addEventListener("click", onSurfaceClick);
  surface.addEventListener("focusout", onFocusOut);
  keyboardInput.addEventListener("blur", releaseAll);
  keyboardInput.addEventListener("beforeinput", onBeforeInput);
  keyboardInput.addEventListener("input", onInput);
  window.addEventListener("blur", releaseAll);
  document.addEventListener("visibilitychange", onVisibilityChange);

  return {
    releaseAll,
    destroy() {
      releaseAll();
      surface.removeEventListener("keydown", onKeyDown);
      surface.removeEventListener("keyup", onKeyUp);
      surface.removeEventListener("click", onSurfaceClick);
      surface.removeEventListener("focusout", onFocusOut);
      keyboardInput.removeEventListener("blur", releaseAll);
      keyboardInput.removeEventListener("beforeinput", onBeforeInput);
      keyboardInput.removeEventListener("input", onInput);
      window.removeEventListener("blur", releaseAll);
      document.removeEventListener("visibilitychange", onVisibilityChange);
    },
  };
}

export function bindPanelKeyButton({
  button,
  tekCode,
  emulator,
  onError,
  shiftOverride,
}) {
  if (!button?.addEventListener) {
    throw new TypeError("button must be a DOM element");
  }
  if (!Number.isInteger(tekCode) || tekCode < 0 || tekCode > 0xff) {
    throw new RangeError("tekCode must be an unsigned byte");
  }
  if (shiftOverride !== undefined && typeof shiftOverride !== "boolean") {
    throw new TypeError("shiftOverride must be a boolean when provided");
  }
  requireFunction(onError, "onError");
  if ((typeof emulator !== "object" && typeof emulator !== "function") || emulator === null) {
    throw new TypeError("emulator is required");
  }

  const document = button.ownerDocument;
  const window = document.defaultView;
  const bindingId = {};
  let pointerId = null;
  let lastModifiers = { shift: false, control: false };

  function invoke(method, ...arguments_) {
    try {
      emulator[method](...arguments_);
      return true;
    } catch (error) {
      onError(error);
      return false;
    }
  }

  function eventModifiers(event) {
    const modifiers = modifiersFrom(event);
    if (shiftOverride === undefined) {
      return modifiers;
    }
    return { ...modifiers, shift: shiftOverride };
  }

  function release(event) {
    if (pointerId === null || (event && event.pointerId !== pointerId)) {
      return;
    }
    const releasedPointerId = pointerId;
    pointerId = null;
    if (activePanelBindings.get(emulator) === bindingId) {
      activePanelBindings.delete(emulator);
    }
    if (event) {
      lastModifiers = eventModifiers(event);
    }
    invoke("panel_release", lastModifiers.shift, lastModifiers.control);

    if (button.hasPointerCapture?.(releasedPointerId)) {
      try {
        button.releasePointerCapture(releasedPointerId);
      } catch (error) {
        onError(error);
      }
    }
  }

  function onPointerDown(event) {
    if (pointerId !== null || event.button !== 0 || !event.isPrimary) {
      return;
    }
    if (activePanelBindings.has(emulator)) {
      event.preventDefault();
      return;
    }
    if (!Number.isInteger(event.pointerId)) {
      onError(new Error("Panel input requires a stable pointer identity"));
      return;
    }
    try {
      button.setPointerCapture(event.pointerId);
    } catch (error) {
      onError(error);
      return;
    }

    lastModifiers = eventModifiers(event);
    if (!invoke("panel_press", tekCode, lastModifiers.shift, lastModifiers.control)) {
      try {
        button.releasePointerCapture(event.pointerId);
      } catch (error) {
        onError(error);
      }
      return;
    }
    pointerId = event.pointerId;
    activePanelBindings.set(emulator, bindingId);
    event.preventDefault();
  }

  function onVisibilityChange() {
    if (document.visibilityState === "hidden") {
      release();
    }
  }

  function onWindowBlur() {
    release();
  }

  button.addEventListener("pointerdown", onPointerDown);
  button.addEventListener("pointerup", release);
  button.addEventListener("pointercancel", release);
  button.addEventListener("lostpointercapture", release);
  window.addEventListener("blur", onWindowBlur);
  document.addEventListener("visibilitychange", onVisibilityChange);

  return {
    release() {
      release();
    },
    destroy() {
      release();
      button.removeEventListener("pointerdown", onPointerDown);
      button.removeEventListener("pointerup", release);
      button.removeEventListener("pointercancel", release);
      button.removeEventListener("lostpointercapture", release);
      window.removeEventListener("blur", onWindowBlur);
      document.removeEventListener("visibilitychange", onVisibilityChange);
    },
  };
}
