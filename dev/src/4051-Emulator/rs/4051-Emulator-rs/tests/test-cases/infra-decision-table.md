# Infra Decision Table

> Draft metadata only. This table is not frozen because the user deferred a final environment-mode decision.

## Metadata

| Field | Value |
|-------|-------|
| Project | Tektronix 4051 Emulator |
| Module | `tek4051-emulator` |
| Migration Scope | Port the browser emulator to native Rust/egui with observed 4051 behavior. |
| Integration Test Environment | `mock` (provisional; user may change later) |
| Created At | 2026-11-12 |
| `infra/` Snapshot | `infra-missing` |
| Status | draft (not frozen) |

## Decision Table

No dependency rows: the reference is a static client-side application. GPIB devices, RTC, tape storage, printer, and audio are simulated in-process. The local browser and file picker are the host test runtime, not provisioned services.

| Dependency | Infra Match | Decision | Auth Method | Reason |
|---|---|---|---|---|

The mode can be changed later. Once the baseline is frozen, changing the mode requires unfreezing the bundle, updating this table, revalidating the baseline, and refreezing it.
