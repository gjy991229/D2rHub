
import { Zap } from "lucide-react";

export function LaunchButton({ count, loading, onClick }: {
  count: number; loading: boolean; onClick: () => void;
}) {
  return (
    <button
      disabled={loading || count === 0}
      onClick={onClick}
      className="primary-cta"
    >
      <Zap size={13} strokeWidth={2} />
      默认启动 ({count})
    </button>
  );
}
