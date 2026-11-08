import { create } from "zustand";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface Confirmation {
  warning: string | null;
  respond: (accepted: boolean) => void;
}

export const useLaunchModConfirmation = create<Confirmation>(() => ({
  warning: null,
  respond: () => {},
}));

let cancelPending: (() => void) | null = null;

export function cancelLaunchModConfirmation() {
  cancelPending?.();
}

export function confirmLaunchMod(warning: string): Promise<boolean> {
  cancelLaunchModConfirmation();
  // Shortcut launches can originate while the main window is hidden or minimized.
  // Fail the launch if revealing the prompt fails, instead of awaiting an invisible decision.
  return new Promise((resolve, reject) => {
    let settled = false;
    const clear = () => {
      settled = true;
      if (cancelPending === cancel) {
        cancelPending = null;
        useLaunchModConfirmation.setState({ warning: null, respond: () => {} });
      }
    };
    const respond = (accepted: boolean) => {
      if (settled) return;
      clear();
      resolve(accepted);
    };
    const cancel = () => respond(false);
    cancelPending = cancel;
    void (async () => {
      const window = getCurrentWindow();
      await window.unminimize();
      if (settled) return;
      await window.show();
      if (settled) return;
      await window.setFocus();
      if (settled) return;
      useLaunchModConfirmation.setState({ warning, respond });
    })().catch(error => {
      if (settled) return;
      clear();
      reject(error);
    });
  });
}
