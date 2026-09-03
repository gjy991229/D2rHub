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
  accept: () => void;
}

export function useApplicationDisclosure(ready: boolean): ApplicationDisclosureState {
  const [checking, setChecking] = useState(true);
  const [required, setRequired] = useState(false);
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    if (!ready) return;
    let cancelled = false;

    void invokeCommand<string>("get_app_version")
      .catch(() => "unknown")
      .then((currentVersion) => {
        if (cancelled) return;
        const normalizedVersion = currentVersion.replace(/^v/i, "").trim() || "unknown";
        setVersion(normalizedVersion);
        setRequired(!hasAcceptedApplicationDisclosure(normalizedVersion));
        setChecking(false);
      });

    return () => {
      cancelled = true;
    };
  }, [ready]);

  const accept = useCallback(() => {
    if (!version) return;
    acceptApplicationDisclosure(version);
    setRequired(false);
  }, [version]);

  return { checking, required, version, accept };
}
