export async function installWebGpuBufferMetrics(page) {
  await page.addInitScript(() => {
    const metrics = {
      available: false,
      active: false,
      elapsedMilliseconds: 0,
      startedAt: 0,
      segmentBufferCreateCalls: 0,
      segmentBufferCreateMilliseconds: 0,
      segmentBufferPeakCreateMilliseconds: 0,
      segmentBufferInitializeCalls: 0,
      segmentBufferInitializeMilliseconds: 0,
      segmentBufferPeakInitializeMilliseconds: 0,
      segmentBufferBytes: 0,
      queueSubmitCalls: 0,
      queueSubmitMilliseconds: 0,
      queueSubmitPeakMilliseconds: 0,
    };
    window.__wasmWebGpuBufferMetrics = metrics;
    document.addEventListener("click", (event) => {
      if (event.target instanceof Element && event.target.closest("#run-pause") && metrics.active) {
        metrics.elapsedMilliseconds = performance.now() - metrics.startedAt;
        metrics.active = false;
      }
    }, true);

    const devicePrototype = globalThis.GPUDevice?.prototype;
    const bufferPrototype = globalThis.GPUBuffer?.prototype;
    const queuePrototype = globalThis.GPUQueue?.prototype;
    const createBuffer = devicePrototype?.createBuffer;
    const unmap = bufferPrototype?.unmap;
    const submit = queuePrototype?.submit;
    const descriptors = [
      [devicePrototype, "createBuffer"],
      [bufferPrototype, "unmap"],
      [queuePrototype, "submit"],
    ].map(([prototype, name]) => (
      prototype && Object.getOwnPropertyDescriptor(prototype, name)
    ));

    if (
      typeof createBuffer !== "function"
      || typeof unmap !== "function"
      || typeof submit !== "function"
      || descriptors.some((descriptor) => (
        !descriptor || (!descriptor.configurable && !descriptor.writable)
      ))
    ) {
      return;
    }

    const replaceMethod = (prototype, name, descriptor, value) => {
      Object.defineProperty(prototype, name, { ...descriptor, value });
    };
    const trackedBuffers = new WeakMap();
    const createBufferDescriptor = descriptors[0];
    const unmapDescriptor = descriptors[1];
    const submitDescriptor = descriptors[2];

    replaceMethod(devicePrototype, "createBuffer", createBufferDescriptor, function (...args) {
      const descriptor = args[0];
      if (
        !metrics.active
        || descriptor?.label !== "Tektronix 4051 vector segment instances"
      ) {
        return createBuffer.apply(this, args);
      }

      const startedAt = performance.now();
      const buffer = createBuffer.apply(this, args);
      const createdAt = performance.now();
      const elapsed = createdAt - startedAt;
      metrics.segmentBufferCreateCalls += 1;
      metrics.segmentBufferCreateMilliseconds += elapsed;
      metrics.segmentBufferPeakCreateMilliseconds = Math.max(
        metrics.segmentBufferPeakCreateMilliseconds,
        elapsed,
      );
      metrics.segmentBufferBytes += descriptor.size;
      trackedBuffers.set(buffer, createdAt);
      return buffer;
    });

    replaceMethod(bufferPrototype, "unmap", unmapDescriptor, function (...args) {
      const createdAt = trackedBuffers.get(this);
      try {
        return unmap.apply(this, args);
      } finally {
        if (createdAt !== undefined) {
          const elapsed = performance.now() - createdAt;
          metrics.segmentBufferInitializeCalls += 1;
          metrics.segmentBufferInitializeMilliseconds += elapsed;
          metrics.segmentBufferPeakInitializeMilliseconds = Math.max(
            metrics.segmentBufferPeakInitializeMilliseconds,
            elapsed,
          );
          trackedBuffers.delete(this);
        }
      }
    });

    replaceMethod(queuePrototype, "submit", submitDescriptor, function (...args) {
      window.__wasmWebGpuMetricsQueue = this;
      if (!metrics.active) {
        return submit.apply(this, args);
      }
      const startedAt = performance.now();
      try {
        return submit.apply(this, args);
      } finally {
        const elapsed = performance.now() - startedAt;
        metrics.queueSubmitCalls += 1;
        metrics.queueSubmitMilliseconds += elapsed;
        metrics.queueSubmitPeakMilliseconds = Math.max(
          metrics.queueSubmitPeakMilliseconds,
          elapsed,
        );
      }
    });
    metrics.available = true;
  });
}

export async function resetWebGpuBufferMetrics(page) {
  await page.evaluate(() => {
    const metrics = window.__wasmWebGpuBufferMetrics;
    metrics.segmentBufferCreateCalls = 0;
    metrics.segmentBufferCreateMilliseconds = 0;
    metrics.segmentBufferPeakCreateMilliseconds = 0;
    metrics.segmentBufferInitializeCalls = 0;
    metrics.segmentBufferInitializeMilliseconds = 0;
    metrics.segmentBufferPeakInitializeMilliseconds = 0;
    metrics.segmentBufferBytes = 0;
    metrics.queueSubmitCalls = 0;
    metrics.queueSubmitMilliseconds = 0;
    metrics.queueSubmitPeakMilliseconds = 0;
    metrics.elapsedMilliseconds = 0;
    metrics.startedAt = performance.now();
    metrics.active = true;
  });
}

export async function readWebGpuBufferMetrics(page) {
  return page.evaluate(() => ({ ...window.__wasmWebGpuBufferMetrics }));
}

export async function measureWebGpuQueueDrain(page) {
  return page.evaluate(async () => {
    const queue = window.__wasmWebGpuMetricsQueue;
    if (!queue) {
      throw new Error("No WebGPU queue submission was observed during the measurement.");
    }
    const startedAt = performance.now();
    await queue.onSubmittedWorkDone();
    return performance.now() - startedAt;
  });
}
