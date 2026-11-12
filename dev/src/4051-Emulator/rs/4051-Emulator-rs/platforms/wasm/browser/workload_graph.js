const WORKLOAD_GAP_SECONDS = 0.05;
const WORKLOAD_HISTORY_WINDOW_SECONDS = 3;

export function retainWorkloadSample(samples, sample) {
  samples.push(sample);
  while (
    samples.length > 1
    && sample.elapsedSeconds - samples[0].elapsedSeconds > WORKLOAD_HISTORY_WINDOW_SECONDS
  ) {
    samples.shift();
  }
}

export function workloadSampleSegments(samples) {
  const segments = [];
  let segmentStart = 0;
  for (let index = 1; index < samples.length; index += 1) {
    const gap = samples[index].elapsedSeconds - samples[index - 1].elapsedSeconds;
    if (gap > WORKLOAD_GAP_SECONDS + 1e-6) {
      segments.push(samples.slice(segmentStart, index));
      segmentStart = index;
    }
  }
  if (segmentStart < samples.length) {
    segments.push(samples.slice(segmentStart));
  }
  return segments;
}

export function workloadPointCoordinates(sample, visibleTop, timeSpan, upperWorkload) {
  return {
    x: Math.max(0, Math.min(sample.workloadPercent, upperWorkload)) / upperWorkload,
    y: (visibleTop - sample.elapsedSeconds) / timeSpan,
  };
}

export function renderWorkloadGraph(canvas, samples, nowSeconds) {
  const bounds = canvas.getBoundingClientRect();
  if (bounds.width <= 0 || bounds.height <= 0) {
    return;
  }

  const pixelRatio = window.devicePixelRatio || 1;
  const width = canvas.clientWidth;
  const height = canvas.clientHeight;
  const pixelWidth = Math.max(1, Math.round(width * pixelRatio));
  const pixelHeight = Math.max(1, Math.round(height * pixelRatio));
  if (canvas.width !== pixelWidth) {
    canvas.width = pixelWidth;
  }
  if (canvas.height !== pixelHeight) {
    canvas.height = pixelHeight;
  }

  const context = canvas.getContext("2d");
  if (!context) {
    throw new Error("The browser could not create the workload graph canvas");
  }
  context.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
  context.clearRect(0, 0, width, height);
  context.fillStyle = "#10151b";
  context.fillRect(0, 0, width, height);

  const plot = {
    left: 44,
    top: 20,
    right: width - 10,
    bottom: height - 38,
  };
  if (plot.right <= plot.left || plot.bottom <= plot.top) {
    return;
  }

  const maxSample = samples.reduce(
    (maximum, sample) => Math.max(maximum, sample.workloadPercent),
    100,
  );
  const upperWorkload = Math.max(120, Math.max(100, maxSample) * 1.1);
  const plotWidth = plot.right - plot.left;
  const plotHeight = plot.bottom - plot.top;
  const xAt = (value) => plot.left + plotWidth * value / upperWorkload;
  const timeSpan = samples.length
    ? Math.max(samples[samples.length - 1].elapsedSeconds - samples[0].elapsedSeconds, 3)
    : 3;

  context.font = "11px system-ui, sans-serif";
  context.lineWidth = 1;
  context.strokeStyle = "#46515d";
  context.beginPath();
  context.moveTo(plot.left, plot.top);
  context.lineTo(plot.left, plot.bottom);
  context.lineTo(plot.right, plot.bottom);
  context.stroke();

  for (let age = 0; age <= timeSpan; age += 1) {
    const y = plot.top + plotHeight * age / timeSpan;
    context.strokeStyle = "#28323c";
    context.beginPath();
    context.moveTo(plot.left, y);
    context.lineTo(plot.right, y);
    context.stroke();
    context.fillStyle = "#aab6c2";
    context.textAlign = "right";
    context.textBaseline = "middle";
    context.fillText(age === 0 ? "now" : `-${age}s`, plot.left - 6, y);
  }

  const thresholdX = xAt(100);
  context.strokeStyle = "#ff7777";
  context.beginPath();
  context.moveTo(thresholdX, plot.top);
  context.lineTo(thresholdX, plot.bottom);
  context.stroke();

  context.fillStyle = "#aab6c2";
  context.textAlign = "left";
  context.textBaseline = "alphabetic";
  context.fillText("0%", plot.left, height - 6);
  context.textAlign = "right";
  context.fillText(`${upperWorkload.toFixed(0)}%`, plot.right, height - 6);
  context.textAlign = "center";
  context.fillText("Workload (%)", (plot.left + plot.right) / 2, height - 6);

  if (samples.length === 0) {
    context.fillStyle = "#aab6c2";
    context.textAlign = "center";
    context.textBaseline = "middle";
    context.fillText("Waiting for worker samples", (plot.left + plot.right) / 2, (plot.top + plot.bottom) / 2);
    return;
  }

  const latestTime = samples[samples.length - 1].elapsedSeconds;
  const elapsedSinceSample = Math.min(Math.max(0, nowSeconds - latestTime), WORKLOAD_GAP_SECONDS);
  const visibleTop = latestTime + elapsedSinceSample;
  const visibleSpan = timeSpan;
  const segments = workloadSampleSegments(samples);

  context.save();
  context.beginPath();
  context.rect(plot.left, plot.top, plotWidth, plotHeight);
  context.clip();
  context.strokeStyle = "#9ecbff";
  context.fillStyle = "#9ecbff";
  context.lineWidth = 1.5;
  context.lineJoin = "round";
  context.lineCap = "round";
  for (const [index, segment] of segments.entries()) {
    const points = segment.map((sample) => {
      const coordinates = workloadPointCoordinates(
        sample,
        visibleTop,
        visibleSpan,
        upperWorkload,
      );
      return {
        x: plot.left + coordinates.x * plotWidth,
        y: plot.top + coordinates.y * plotHeight,
      };
    });
    if (index + 1 === segments.length && points[points.length - 1].y > plot.top) {
      const last = points[points.length - 1];
      points.push({ x: last.x, y: plot.top });
    }
    if (points.length === 1) {
      context.beginPath();
      context.arc(points[0].x, points[0].y, 2, 0, Math.PI * 2);
      context.fill();
    } else {
      context.beginPath();
      context.moveTo(points[0].x, points[0].y);
      for (const point of points.slice(1)) {
        context.lineTo(point.x, point.y);
      }
      context.stroke();
    }
  }
  context.restore();

  context.fillStyle = "#ff7777";
  context.textAlign = "center";
  context.textBaseline = "top";
  context.fillText("100%", thresholdX, plot.top + 2);
}
