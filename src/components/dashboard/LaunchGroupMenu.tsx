import { ChevronRight, ListChecks } from "lucide-react";

import { useI18n } from "../../i18n";

interface LaunchGroupMenuProps {
  count: number;
  open: boolean;
  disabled?: boolean;
  onToggle: () => void;
}

export function LaunchGroupMenu({ count, open, disabled = false, onToggle }: LaunchGroupMenuProps) {
  const { t } = useI18n();
  return (
    <button
      type="button"
      className="control-btn launch-group-trigger min-w-[86px]"
      aria-expanded={open}
      aria-controls={open ? "launch-group-panel" : undefined}
      disabled={disabled}
      onClick={onToggle}
    >
      <ListChecks size={13} strokeWidth={1.9} aria-hidden="true" />
      <span>{t("launch.scheme.label")}</span>
      {count > 0 && <span className="launch-group-trigger-count">{count}</span>}
      <ChevronRight className="launch-group-trigger-chevron" size={11} strokeWidth={2} aria-hidden="true" />
    </button>
  );
}
