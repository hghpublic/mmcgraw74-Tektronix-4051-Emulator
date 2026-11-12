export function bindBasicInputStatusUi() {
  const elements = {
    indicator: document.querySelector("#basic-input-status"),
    promptStatus: document.querySelector("#basic-prompt-status"),
    inputNotice: document.querySelector("#basic-prompt-notice"),
  };
  for (const [name, element] of Object.entries(elements)) {
    if (!element) {
      throw new Error(`BASIC input status UI is missing its ${name} element`);
    }
  }

  return {
    update(snapshot) {
      const promptDetected = snapshot.running
        && snapshot.basic_prompt_ready
        && !snapshot.runtime_error;
      elements.indicator.classList.toggle("is-disabled", !promptDetected);
      elements.indicator.classList.toggle("is-ready", promptDetected);
      elements.indicator.setAttribute(
        "aria-label",
        !snapshot.running
          ? "Machine stopped"
          : promptDetected
            ? "BASIC prompt detected"
            : "BASIC prompt not detected",
      );
      elements.promptStatus.textContent = snapshot.runtime_error
        ? "Reset the emulator before the machine can process keyboard input."
        : !snapshot.running
          ? "The emulator is stopped; keyboard input is not delivered."
          : promptDetected
            ? "BASIC prompt detected; host keyboard input reaches the machine."
            : "BASIC prompt not detected; host keyboard input still reaches the machine.";
      elements.inputNotice.textContent = snapshot.input_notice ?? "";
      elements.inputNotice.hidden = !snapshot.input_notice;
    },
  };
}
