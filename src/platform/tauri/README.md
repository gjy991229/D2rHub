# Tauri frontend boundary

Frontend features import `invokeCommand`, `listenEvent`, and `emitEvent` from
this directory instead of depending directly on Tauri's core/event modules.

- `contracts.ts` is the allowlist for command and application-event names.
- `client.ts` preserves the existing wire names, argument casing, and generic
  result typing while normalizing bridge failures.
- `errors.ts` adds operation metadata without changing the text shown by
  existing `String(error)` call sites.
- `platformBoundary.test.ts` prevents new direct core/event imports outside
  this adapter.

Feature-specific gateways can be added above this layer as modules are
extracted. They should reuse this client rather than import Tauri directly.

The architecture test rejects direct core/event imports everywhere outside
this adapter, including the settings shell and capability panels.
