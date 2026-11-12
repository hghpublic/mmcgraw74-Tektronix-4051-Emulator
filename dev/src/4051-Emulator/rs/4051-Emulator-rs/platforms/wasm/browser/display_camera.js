const MIN_ZOOM = 0.25;
const MAX_ZOOM = 8;
const WHEEL_ZOOM_RATE = 0.001;

export function bindDisplayCamera(surface, onChange = () => {}) {
  if (!surface?.addEventListener || typeof surface.querySelectorAll !== "function") {
    throw new TypeError("surface must be a display element");
  }
  if (typeof onChange !== "function") {
    throw new TypeError("onChange must be a function");
  }

  const canvases = [...surface.querySelectorAll("canvas")];
  if (canvases.length === 0) {
    throw new TypeError("surface must contain at least one display canvas");
  }

  let zoom = 1;
  let offsetX = 0;
  let offsetY = 0;
  let drag = null;
  let vectorPresentation = false;

  function getViewTransform() {
    const canvas = canvases.find((candidate) => !candidate.hidden) ?? canvases[0];
    const widthScale = canvas.clientWidth > 0 ? canvas.width / canvas.clientWidth : 1;
    const heightScale = canvas.clientHeight > 0 ? canvas.height / canvas.clientHeight : 1;
    return {
      zoom,
      panX: offsetX * widthScale,
      panY: offsetY * heightScale,
    };
  }

  function applyTransform() {
    const transform = zoom === 1 && offsetX === 0 && offsetY === 0
      ? ""
      : `translate(${offsetX}px, ${offsetY}px) scale(${zoom})`;
    for (const canvas of canvases) {
      canvas.style.transform = vectorPresentation && canvas.id === "screen-webgpu"
        ? ""
        : transform;
    }
    onChange(getViewTransform());
  }

  function reset() {
    zoom = 1;
    offsetX = 0;
    offsetY = 0;
    drag = null;
    surface.classList.remove("is-panning");
    applyTransform();
  }

  function onWheel(event) {
    const deltaScale = event.deltaMode === 1
      ? 16
      : event.deltaMode === 2
        ? surface.clientHeight
        : 1;
    const nextZoom = Math.min(
      MAX_ZOOM,
      Math.max(MIN_ZOOM, zoom * Math.exp(-event.deltaY * deltaScale * WHEEL_ZOOM_RATE)),
    );
    if (nextZoom !== zoom) {
      const bounds = surface.getBoundingClientRect();
      const pointerX = event.clientX - (bounds.left + bounds.width / 2);
      const pointerY = event.clientY - (bounds.top + bounds.height / 2);
      const ratio = nextZoom / zoom;
      offsetX = pointerX - (pointerX - offsetX) * ratio;
      offsetY = pointerY - (pointerY - offsetY) * ratio;
      zoom = nextZoom;
      applyTransform();
    }
    event.preventDefault();
  }

  function onPointerDown(event) {
    if (event.button !== 0 || !event.isPrimary) {
      return;
    }
    drag = {
      pointerId: event.pointerId,
      clientX: event.clientX,
      clientY: event.clientY,
      offsetX,
      offsetY,
    };
    surface.setPointerCapture(event.pointerId);
    surface.classList.add("is-panning");
  }

  function onPointerMove(event) {
    if (drag?.pointerId !== event.pointerId) {
      return;
    }
    offsetX = drag.offsetX + event.clientX - drag.clientX;
    offsetY = drag.offsetY + event.clientY - drag.clientY;
    applyTransform();
  }

  function onPointerEnd(event) {
    if (drag?.pointerId !== event.pointerId) {
      return;
    }
    drag = null;
    surface.classList.remove("is-panning");
  }

  function onContextMenu(event) {
    event.preventDefault();
    reset();
  }

  function setVectorPresentation(enabled) {
    if (typeof enabled !== "boolean") {
      throw new TypeError("enabled must be a boolean");
    }
    vectorPresentation = enabled;
    applyTransform();
  }

  surface.addEventListener("wheel", onWheel, { passive: false });
  surface.addEventListener("pointerdown", onPointerDown);
  surface.addEventListener("pointermove", onPointerMove);
  surface.addEventListener("pointerup", onPointerEnd);
  surface.addEventListener("pointercancel", onPointerEnd);
  surface.addEventListener("lostpointercapture", onPointerEnd);
  surface.addEventListener("contextmenu", onContextMenu);

  return {
    getViewTransform,
    setVectorPresentation,
  };
}
