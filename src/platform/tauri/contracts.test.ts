import {
  isTauriCommandName,
  isTauriEventName,
  TAURI_COMMANDS,
  TAURI_EVENTS,
} from "./contracts";

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(`FAIL: ${message}`);
  console.log(`PASS: ${message}`);
}

assert(
  new Set(TAURI_COMMANDS).size === TAURI_COMMANDS.length,
  "the frontend command contract has no duplicate names",
);
assert(
  new Set(TAURI_EVENTS).size === TAURI_EVENTS.length,
  "the frontend event contract has no duplicate names",
);
assert(isTauriCommandName("launch_accounts"), "known commands pass runtime validation");
assert(
  isTauriCommandName("get_capability_statuses"),
  "the capability status snapshot command is part of the frontend contract",
);
assert(
  isTauriCommandName("room_automation_save_config"),
  "the room automation sidecar save command is part of the frontend contract",
);
assert(!isTauriCommandName("launch_account"), "unknown commands fail runtime validation");
assert(isTauriEventName("launch-progress"), "known events pass runtime validation");
assert(
  isTauriEventName("capability-status-updated"),
  "the capability status commit event is part of the frontend contract",
);
assert(
  isTauriEventName("room-automation://status-changed"),
  "the room automation status event is part of the frontend contract",
);
