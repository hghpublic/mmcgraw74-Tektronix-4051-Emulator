export async function installWebGpuSurfaceMetrics(page) {
  await page.addInitScript(() => {
    const metrics = {
      available: false,
      presentationCalls: 0,
      totalPresentationMilliseconds: 0,
      peakPresentationMilliseconds: 0,
      skippedPresentations: 0,
      revisionChanges: 0,
      presentedRevisions: 0,
      lastFrameRevision: null,
      acquisitions: 0,
      totalMilliseconds: 0,
      peakMilliseconds: 0,
    };
    window.__wasmWebGpuSurfaceMetrics = metrics;

    const prototype = globalThis.GPUCanvasContext?.prototype;
    const getCurrentTexture = prototype?.getCurrentTexture;
    if (typeof getCurrentTexture !== "function") {
      return;
    }

    const descriptor = Object.getOwnPropertyDescriptor(prototype, "getCurrentTexture");
    if (descriptor && !descriptor.configurable && !descriptor.writable) {
      return;
    }
    Object.defineProperty(prototype, "getCurrentTexture", {
      ...descriptor,
      configurable: true,
      writable: true,
      value: function (...arguments_) {
        const startedAt = performance.now();
        try {
          return getCurrentTexture.apply(this, arguments_);
        } finally {
          const elapsed = performance.now() - startedAt;
          metrics.acquisitions += 1;
          metrics.totalMilliseconds += elapsed;
          metrics.peakMilliseconds = Math.max(metrics.peakMilliseconds, elapsed);
        }
      },
    });
    metrics.available = true;
  });
}

export async function resetWebGpuSurfaceMetrics(page) {
  await page.evaluate(() => {
    const metrics = window.__wasmWebGpuSurfaceMetrics;
    metrics.presentationCalls = 0;
    metrics.totalPresentationMilliseconds = 0;
    metrics.peakPresentationMilliseconds = 0;
    metrics.skippedPresentations = 0;
    metrics.revisionChanges = 0;
    metrics.presentedRevisions = 0;
    metrics.acquisitions = 0;
    metrics.totalMilliseconds = 0;
    metrics.peakMilliseconds = 0;
  });
}

export async function readWebGpuSurfaceMetrics(page) {
  return page.evaluate(() => ({ ...window.__wasmWebGpuSurfaceMetrics }));
}
