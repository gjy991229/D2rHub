import type { ReactNode } from "react";
import { Modal } from "../../components/ui/Modal";
import type { GlobalConfig } from "../../store/types";
import { SettingsNavigation } from "./SettingsNavigation";
import type { SettingsTabId } from "./settingsRegistry";

interface SettingsShellProps {
  open: boolean;
  title: string;
  activeTab: SettingsTabId;
  config: GlobalConfig | null;
  onClose: () => void;
  onTabChange: (tab: SettingsTabId) => void;
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
  return (
    <Modal open={open} onClose={onClose} title={title} width="max-w-[1020px]" closeOnContextMenu>
      <div className="settings-center-shell flex h-[640px] max-h-[calc(100vh-96px)] flex-col">
        <div className="settings-center-layout">
          <SettingsNavigation
            activeTab={activeTab}
            config={config}
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
