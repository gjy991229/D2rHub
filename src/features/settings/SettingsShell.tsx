import { useEffect, useState, type ReactNode } from "react";
import { Modal } from "../../components/ui/Modal";
import type { CapabilityStatusSnapshot, GlobalConfig } from "../../store/types";
import { initCapabilityStatusSync } from "../capabilities";
import { SettingsNavigation } from "./SettingsNavigation";
import { OptionalFeaturesNavigation } from "./OptionalFeaturesNavigation";
import { isMinimalMode } from "../profile/featureProfile";
import {
  isOptionalSettingsTab,
  type OptionalModuleTabId,
  type SettingsTabId,
} from "./settingsRegistry";

interface SettingsShellProps {
  open: boolean;
  title: string;
  activeTab: SettingsTabId;
  config: GlobalConfig | null;
  installedModules: readonly OptionalModuleTabId[];
  onClose: () => void;
  onTabChange: (tab: SettingsTabId) => boolean | void;
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
  const optionalFeatureActive = !minimalMode && isOptionalSettingsTab(activeTab);

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
      dismissible={dismissible}
    >
      <div className="settings-center-shell flex flex-col">
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
            {optionalFeatureActive && (
              <OptionalFeaturesNavigation
                activeTab={activeTab}
                config={config}
                capabilityStatus={capabilityStatus}
                capabilityStatusUnavailable={capabilityStatusUnavailable}
                language={config?.app_language}
                installedModules={installedModules}
                onSelect={onTabChange}
              />
            )}
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
      </div>
    </Modal>
  );
}
