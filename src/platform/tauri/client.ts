import { invoke, type InvokeArgs } from "@tauri-apps/api/core";
import {
  emit,
  listen,
  type EventCallback,
  type Options,
  type UnlistenFn,
} from "@tauri-apps/api/event";
import type { TauriCommandName, TauriEventName } from "./contracts";
import { normalizeTauriError } from "./errors";

export async function invokeCommand<TResult = void>(
  command: TauriCommandName,
  args?: InvokeArgs,
): Promise<TResult> {
  try {
    return await invoke<TResult>(command, args);
  } catch (error) {
    throw normalizeTauriError("command", command, error);
  }
}

export async function listenEvent<TPayload>(
  eventName: TauriEventName,
  handler: EventCallback<TPayload>,
  options?: Options,
): Promise<UnlistenFn> {
  try {
    return await listen<TPayload>(eventName, handler, options);
  } catch (error) {
    throw normalizeTauriError("event-listen", eventName, error);
  }
}

export async function emitEvent<TPayload = void>(
  eventName: TauriEventName,
  payload?: TPayload,
): Promise<void> {
  try {
    await emit(eventName, payload);
  } catch (error) {
    throw normalizeTauriError("event-emit", eventName, error);
  }
}
