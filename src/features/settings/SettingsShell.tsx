import { useEffect, useState, type ReactNode } from "react";
import { Modal } from "../../components/ui/Modal";
import type { CapabilityStatusSnapshot, GlobalConfig } from "../../store/types";
import { initCapabilityStatusSync } from "../capabilities";
import { SettingsNavigation } from "./SettingsNavigation";
import { isMinimalMode } from "../profile/featureProfile";
import {
  SETTINGS_COPY,
  type OptionalModuleTabId,
  type SettingsTabId,
} from "./settingsRegistry";
import "./navigation.css";

interface SettingsShellProps {
  open: boolean;
  title: string;
  activeTab: SettingsTabId;
  config: GlobalConfig | null;
  installedModules: readonly OptionalModuleTabId[];
  onClose: () => void;
  onTabChange: (tab: SettingsTabId) => boolean | void | Promise<boolean | void>;
  dismissible?: boolean;
  children: ReactNode;
}

export function SettingsShell({
  open,
  title,
  activeTab,
  config,
  installedModules,
  onClose,
  onTabChange,
  dismissible = true,
  children,
}: SettingsShellProps) {
  const [capabilityStatus, setCapabilityStatus] = useState<CapabilityStatusSnapshot | null>(null);
  const [capabilityStatusUnavailable, setCapabilityStatusUnavailable] = useState(false);
  const minimalMode = isMinimalMode(config);
  const pageCopy = SETTINGS_COPY[config?.app_language === "en-US" ? "en-US" : "zh-CN"][activeTab];
  const hasOwnHeading = ["module-management", "mod-processing", "room-automation", "tasks"].includes(activeTab);

  useEffect(() => {
    if (!open || minimalMode) {
      setCapabilityStatus(null);
      setCapabilityStatusUnavailable(false);
      return;
    }

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
  }, [minimalMode, open]);

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={title}
      width="settings-modal-size"
      closeOnContextMenu
      closeOnBackdropClick={false}
      dismissible={dismissible}
    >
      <div className="settings-center-shell settings-workspace flex flex-col">
        <div className="settings-center-layout">
          <SettingsNavigation
            activeTab={activeTab}
            config={config}
            capabilityStatus={capabilityStatus}
            capabilityStatusUnavailable={capabilityStatusUnavailable}
            language={config?.app_language}
            installedModules={installedModules}
            onSelect={onTabChange}
          />
          <div className="settings-panel-column">
            <div
              key={activeTab}
              id={`settings-panel-${activeTab}`}
              role="tabpanel"
              aria-labelledby={`settings-tab-${activeTab}`}
              className="settings-panel-scroll space-y-3"
            >
              {!hasOwnHeading && (
                <header className="settings-page-heading">
                  <h2>{pageCopy.label}</h2>
                  <p>{pageCopy.description}</p>
                </header>
              )}
              {children}
            </div>
          </div>
        </div>
      </div>
    </Modal>
  );
}
