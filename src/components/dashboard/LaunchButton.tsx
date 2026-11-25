
import { Button } from "../ui/Button";
import { useI18n } from "../../i18n";
import { Play } from "lucide-react";

export function LaunchButton({ count, loading, onClick, selected = false }: {
  count: number; loading: boolean; onClick: () => void; selected?: boolean;
}) {
  const en = useI18n().language === "en-US";
  return (
    <Button type="button" variant="primary" size="sm"
      disabled={loading || count === 0}
      onClick={onClick}
    >
      <Play size={12} fill="currentColor" aria-hidden="true" />
      {selected ? (en ? "Launch selected" : "启动选中") : (en ? "Launch all" : "启动全部")}
    </Button>
  );
}
