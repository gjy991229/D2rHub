# ADR 0003: Pure runtime and transactional mode restart

- Status: Accepted
- Date: 2026-09-08

## Decision

Keep the persisted `minimal` / `normal` profile values and all existing module
preferences. Present `minimal` as Pure mode. It retains multi-instance operation,
window shortcuts and Mod management; it does not promise to launch an unmodified
game or exclude optional code from the executable.

Pure startup does not install capability drivers or the room automation manager,
initialize room automation sidecars, start the capability supervisor, or create
auxiliary WebViews. Configuration, account recovery, launch leases and the main
window lifecycle remain core services. The empty application registry is only a
control-plane value and owns no optional runtime resources.

Input services are demand-driven in both profiles. Core shortcuts require only
the keyboard hook; room shortcuts subscribe while enabled. Mouse hooks exist
only for visible pet input or statistics-window interaction. Their forwarded events
share a bounded queue, active whenever either consumer needs it. Statistics
double-click and hover events do not depend on enabling the pet. With no consumers,
the hooks and their worker threads are released. Configuration observers enqueue
coalesced updates on the shared blocking executor instead of running native
lifecycle operations inside the configuration transaction. No input supervisor
thread is retained when idle.

Keyboard, mouse and event-forwarding workers have independent ownership. An
optional mouse/forwarder failure cannot remove an existing core keyboard hook.
Keyboard retirement is serialized with shortcut dispatch and waits for consumed
keys to be released. Startup timeouts retain cancellable ownership of late workers.

Persisted room shortcuts are consulted when core keys are edited, even in Pure
mode, without constructing a room runtime. Existing conflicts leave the room
settings manager available for repair; activation still validates and refuses
conflicting shortcuts.

Pure mode does not scan the Mod catalog on dashboard startup. Opening the Mod
picker or manager requests a scan, with loading/error feedback and an explicit
retry. The old duplicate backend startup scan is removed.
Unfinished argument-update journals are recovered separately before activation
and at launch entry points, with no installation scan or missing-sidecar creation.

## Profile transition

The initial profile choice commits without restarting because no optional runtime
has been installed. Subsequent mode changes use a fresh process:

1. Save pending frontend settings, flush the latest window geometry, and serialize against activation and explicit
   optional starts.
2. Suspend optional intent, stop supervised and explicit activity, wait for
   cleanup, and destroy auxiliary windows.
3. Reserve Mod/catalog/account/task/window-write admission and host/build leases.
   Reservations are admission flags, not permanently held mutexes. Already
   accepted writes finish before admission closes; later commands return without
   blocking the main event loop. On failure, reopen admission.
4. Start a replacement process. Send the account/PID/launch-argument snapshot
   through a private inherited pipe. The child opens the predecessor process
   handle, receives the snapshot and acknowledges readiness through a unique
   local event. It has not opened a WebView or read configuration yet.
   Creation, bounded snapshot transfer and readiness share a ten-second deadline.
   Timeout terminates the reader and cancels the dedicated worker's synchronous
   I/O; late process creation observes cancellation and cannot activate the child.
5. Commit only the profile through the existing durable configuration transaction.
   Do not publish the replacement's profile to old WebViews or resume old drivers.
6. Keep write admission closed, return a restart outcome to the frontend and exit.
   The already-prepared exit worker requests normal application exit; no
   watchdog forcibly terminates the app to escape a permanently locked IPC.
7. The child waits for the predecessor to exit before the single-instance check.
   Restore game-instance identity only if PID and process creation time still
   match. Game processes remain running; automation workflows are not replayed.

An uncommitted restart object terminates its waiting child on drop. Failure to
prepare the child, drain optional work, reserve writers or save the profile does
not report a completed switch. Guards reopen admission and configuration intent
is reapplied; already-cancelled automation is not restarted automatically.

## Scope

This is a runtime-resource boundary within the existing first-party modular
monolith, not a dynamic DLL/plugin loader. Optional modules in Normal mode retain
their existing lifecycle supervision. Existing configuration schema values and
module data remain compatible.
