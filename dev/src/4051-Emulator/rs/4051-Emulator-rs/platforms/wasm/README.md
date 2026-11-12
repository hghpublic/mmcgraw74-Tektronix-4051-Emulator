# WASM WebGPU renderers

The browser shell uses the existing WebGPU vector renderer by default. Add
`?renderer=extended` to the browser URL to opt into the Extended WebGPU renderer.
The extended renderer starts with a linear one-second glow decay at x1; the
selected CPU speed scales decay, including while paused. The added brightness
starts near white, then fades into the exact steady-state image produced by the
standard WebGPU renderer.
`DecayParameters` also supports exponential and inverse-power curves with an
explicit threshold for extinguishing the glow.

The CPU reference renderer remains responsible for browser screen-copy data;
synchronous GPU readback is not available in the browser.
