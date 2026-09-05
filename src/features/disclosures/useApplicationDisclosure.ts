import { useCallback, useEffect, useState } from "react";
import { invokeCommand } from "../../platform/tauri";
import {
  acceptApplicationDisclosure,
  hasAcceptedApplicationDisclosure,
} from "./disclosureStorage";

interface ApplicationDisclosureState {
  checking: boolean;
  required: boolean;
  version: string | null;
  accepting: boolean;
  accept: () => Promise<void>;
}

export function useApplicationDisclosure(
  ready: boolean,
  runtimeReady = true,
): ApplicationDisclosureState {
  const [checking, setChecking] = useState(true);
  const [required, setRequired] = useState(false);
  const [version, setVersion] = useState<string | null>(null);
  const [accepting, setAccepting] = useState(false);
  const [acceptedInSession, setAcceptedInSession] = useState(false);

  useEffect(() => {
    if (!ready) return;
    let cancelled = false;

    void (async () => {
      const currentVersion = await invokeCommand<string>("get_app_version").catch(() => "unknown");
      if (cancelled) return;

      const normalizedVersion = currentVersion.replace(/^v/i, "").trim() || "unknown";
      setVersion(normalizedVersion);
      const accepted = normalizedVersion === "unknown"
        ? acceptedInSession
        : hasAcceptedApplicationDisclosure(normalizedVersion);
      if (!accepted) {
        setRequired(true);
        setChecking(false);
        return;
      }

      if (!runtimeReady) {
        setRequired(false);
        setChecking(false);
        return;
      }

      setChecking(true);
      try {
        await invokeCommand<boolean>("activate_application_runtime");
        if (!cancelled) setRequired(false);
      } catch (error) {
        console.error("Failed to activate application runtime:", error);
        if (!cancelled) setRequired(true);
      } finally {
        if (!cancelled) setChecking(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [acceptedInSession, ready, runtimeReady]);

  const accept = useCallback(async () => {
    if (!version || accepting) return;
    setAccepting(true);
    try {
      if (runtimeReady) {
        await invokeCommand<boolean>("activate_application_runtime");
      }
      if (version === "unknown") setAcceptedInSession(true);
      else acceptApplicationDisclosure(version);
      setRequired(false);
    } finally {
      setAccepting(false);
    }
  }, [accepting, runtimeReady, version]);

  return { checking, required, version, accepting, accept };
}
