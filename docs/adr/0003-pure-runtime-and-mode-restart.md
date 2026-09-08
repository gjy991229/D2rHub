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

Business shortcuts use one Windows RegisterHotKey / WM_HOTKEY message thread,
with MOD_NOREPEAT and native conflict reporting. Core shortcuts remain available
in Pure mode; room shortcuts are registered only while that capability is active.
Recording and scoped automatic input temporarily unregister hotkeys and restore
the current route snapshot afterward. Hotkeys are unregistered on shutdown.

Desktop-pet input is independent: one passive listener thread owns both keyboard
and mouse hooks, and a bounded queue forwards animation events. The listener is
created only for an enabled, visible pet and retired when it is no longer needed.
Its callbacks always pass input onward. Startup timeouts retain cancellable
ownership of late workers. Pet failures cannot remove business hotkeys.

Both statistics and TZ mini windows use local double-click, Enter, pointer drag,
and resize events. Statistics mini mode no longer enables click-through, and
neither overlay needs a global mouse hook.
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
