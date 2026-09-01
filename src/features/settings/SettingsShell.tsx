import { useEffect, useState, type ReactNode } from "react";
import { Modal } from "../../components/ui/Modal";
import type { CapabilityStatusSnapshot, GlobalConfig } from "../../store/types";
import { initCapabilityStatusSync } from "../capabilities";
import { SettingsNavigation } from "./SettingsNavigation";
import type { SettingsTabId } from "./settingsRegistry";

interface SettingsShellProps {
  open: boolean;
  title: string;
  activeTab: SettingsTabId;
  config: GlobalConfig | null;
  onClose: () => void;
  onTabChange: (tab: SettingsTabId) => boolean | void;
  children: ReactNode;
}

export function SettingsShell({
  open,
  title,
  activeTab,
  config,
  onClose,
  onTabChange,
  children,
}: SettingsShellProps) {
  const [capabilityStatus, setCapabilityStatus] = useState<CapabilityStatusSnapshot | null>(null);
  const [capabilityStatusUnavailable, setCapabilityStatusUnavailable] = useState(false);

  useEffect(() => {
    if (!open) return;

    let disposed = false;
    let stopListening: (() => void) | undefined;
    setCapabilityStatus(null);
    setCapabilityStatusUnavailable(false);

    void initCapabilityStatusSync((snapshot) => {
      if (!disposed) setCapabilityStatus(snapshot);
    }).then((stop) => {
      if (disposed) stop();
      else stopListening = stop;
    }).catch((error) => {
      console.error("Failed to synchronize capability status:", error);
      if (!disposed) {
        setCapabilityStatus(null);
        setCapabilityStatusUnavailable(true);
      }
    });

    return () => {
      disposed = true;
      stopListening?.();
    };
  }, [open]);

  return (
    <Modal open={open} onClose={onClose} title={title} width="max-w-[1020px]" closeOnContextMenu>
      <div className="settings-center-shell flex h-[640px] max-h-[calc(100vh-96px)] flex-col">
        <div className="settings-center-layout">
          <SettingsNavigation
            activeTab={activeTab}
            config={config}
            capabilityStatus={capabilityStatus}
            capabilityStatusUnavailable={capabilityStatusUnavailable}
            language={config?.app_language}
            onSelect={onTabChange}
          />
          <div
            id={`settings-panel-${activeTab}`}
            role="tabpanel"
            aria-labelledby={`settings-tab-${activeTab}`}
            className="settings-panel-scroll space-y-3"
          >
            {children}
          </div>
        </div>
      </div>
    </Modal>
  );
}
