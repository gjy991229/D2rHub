# ADR 0002: Stable multi-instance core with optional capability modules

- Status: Accepted
- Date: 2026-09-01

## Context

D2RHub is first and foremost a multi-instance orchestrator for Diablo II:
Resurrected. Account isolation, launch preflight, host-resource serialization,
process lifecycle, and recovery are product invariants. Features such as the
overlay, Terror Zone information, statistics, audio telemetry, shortcuts,
desktop pet, and room automation add value, but a failure in one of them must
not prevent the core multi-instance workflow from starting or shutting down.

The current implementation groups code mostly by technical location. Large
Tauri command modules and React screens consequently own domain rules, storage,
runtime tasks, IPC, and presentation at the same time. The settings center is a
manual composition of every feature, so adding a feature increases coupling in
both the front end and the back end.

The application runs elevated on Windows and manipulates machine-wide
Battle.net state. Loading arbitrary third-party code in-process would expand
the trusted computing base and make lifecycle failures harder to contain.

## Decision

D2RHub uses a **modular monolith with a stable core and statically registered
first-party capability modules**. The design uses plugin-shaped contracts, but
does not load arbitrary DLLs or scripts into the elevated process.

### Product layers

1. **Multi-instance core** (always enabled)
   - Account identity and per-account state isolation.
   - Launch-context resolution and no-side-effect preflight.
   - Account and host runtime leases.
   - Instance launch, cancellation, shutdown, and recovery.
2. **Platform services** (required infrastructure)
   - Versioned configuration loading, migration, validation, and atomic writes.
   - IPC commands/events, logging, operating-system adapters, and task
     supervision.
   - These services are not user-facing modules and cannot be disabled.
3. **Capability modules** (independently switchable)
   - Overlay, Terror Zone, statistics, audio telemetry, shortcuts, desktop pet,
     and automation, including room-follow and room-rotation functionality.
   - A disabled module owns no background task, global listener, window, or
     machine resource.
4. **Control surface**
   - The dashboard presents core instance operations.
   - The settings center discovers registered settings sections and module
     panels; it does not implement their business rules.

### Dependency direction

```text
interface (Tauri commands, React screens)
                 |
                 v
application (use cases, module lifecycle, orchestration)
                 |
                 v
domain (accounts, launch policy, configuration schema)
                 ^
                 |
infrastructure (filesystem, registry, processes, windows, HTTP)
```

- Domain code cannot import Tauri commands, React concepts, or operating-system
  implementations.
- Tauri commands are thin adapters. They validate IPC input, call one
  application use case, and map its result to an IPC response.
- Infrastructure implements ports owned by the application/domain layers.
- Feature modules may depend on public core/application ports and declared
  platform capabilities. They cannot call another module's private commands or
  reach into its store.
- Frontend code accesses Tauri only through the platform gateway. Stores and
  components do not import raw `invoke`, `listen`, or `emit` APIs.

### Capability module contract

Every optional module has one stable identifier and declares:

- metadata and settings navigation contribution;
- required core/platform capabilities and optional module dependencies;
- whether it is enabled by default;
- typed configuration projection plus schema version and migrations;
- lifecycle hooks for start, stop, configuration changes, and health;
- owned commands, events, background tasks, windows, and cleanup actions.

Runtime state is explicit: `disabled`, `stopped`, `starting`, `running`,
`degraded`, or `failed`. Optional-module failures are reported but isolated from
the multi-instance core. Start and stop operations are idempotent.

The first implementation uses a static registry compiled with D2RHub. A future
out-of-process extension protocol may be added for untrusted integrations, but
it is not part of this decision.

## Configuration compatibility

The existing global configuration remains the compatibility envelope during
the refactor. Moving a setting to a module does **not** immediately move or
rename its persisted field.

Configuration changes follow these rules:

1. Every historical supported format is accepted by the newest loader.
2. Missing values receive the same defaults as the historical implementation;
   unknown values never crash startup.
3. Migration is deterministic and idempotent: loading an already-normalized
   configuration produces no further semantic change.
4. Validation completes before module startup or host side effects.
5. An upgrade writes through the existing staging/backup transaction and keeps
   a recoverable pre-upgrade copy.
6. Module code reads typed projections, while a compatibility adapter maps
   those projections to the legacy fields.
7. New module-only metadata must not require an old binary to understand it.
   Until downgrade support is intentionally retired, existing fields remain in
   the legacy envelope and new module metadata is stored in a versioned module
   sidecar. If a change must extend the global envelope, it increments the
   global schema version so older binaries fail closed before rewriting the
   file. Same-version unknown fields are not a supported extension mechanism.

Compatibility is guarded by fixtures for old, partial, current, and
future-additive JSON. Tests assert deserialization, defaults, normalization,
and repeat-load stability. A schema version is not considered a migration by
itself; the observable values and side effects are the contract.

## Settings composition

The settings shell owns only:

- open/close behavior and responsive layout;
- navigation, search, keyboard focus, and unsaved-state coordination;
- module availability, enable/disable state, and health summaries;
- save/error feedback shared by all sections.

Each core/platform section or optional capability supplies its own descriptor
and panel. A panel owns its fields, validation, typed localized copy, help text,
immediate side effects, and tests. New modules must ship their `zh-CN` and
`en-US` copy with the module; the global DOM-translation observer is a legacy
compatibility layer, not an extension API. Navigation groups communicate
architecture without exposing implementation jargon to users:
**Multi-instance**, **Application**, and **Optional features**.

## Room automation integration

The unfinished room-follow/room-rotation work at
`095a6e7e78b1693b202d44ec699a25a10700fb25` is not merged during the foundation
refactor. When complete, it enters as an optional `room-automation` capability:

- it depends on public account selection, launch, and instance-status ports;
- its scheduler and listeners are owned by its lifecycle and stop when disabled;
- its settings panel is registered rather than appended to the settings shell;
- its configuration has a legacy adapter if the feature branch has already
  shipped a persisted format;
- it cannot mutate launch internals or global configuration through command
  module implementation details.

## Consequences

- The multi-instance workflow remains available even when an optional feature
  is disabled or degraded.
- Features can be tested and evolved through explicit contracts without a
  runtime plugin security model.
- The settings center scales by registration instead of accumulating conditional
  JSX and cross-feature effects.
- Migration is incremental. Existing large modules remain functional while
  application services and capability boundaries replace them one vertical
  slice at a time.
- Some compatibility adapters temporarily duplicate representations. They are
  intentional and may only be removed through a separately documented support
  decision.
