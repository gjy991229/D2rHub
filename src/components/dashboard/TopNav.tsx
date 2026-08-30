
import { Settings, Info, Minus, BookOpen, BarChart3, Share2 } from "lucide-react";

export function TopNav({
  onAbout, onExit, onOpenConfig, onHelp, onStats, onShareReport, sharingReport,
}: {
  onAbout: () => void;
  onExit: () => void;
  onOpenConfig: () => void;
  onHelp: () => void;
  onStats: () => void;
  onShareReport: () => void;
  sharingReport: boolean;
}) {
  return (
    <div
      className="top-nav-material shrink-0 flex items-center px-4 select-none"
      data-tauri-drag-region
    >
      <div className="flex items-center gap-2.5 mr-5" data-tauri-drag-region>
        <span className="swiss-mark">
          <img src="/logo.png" alt="D2RHub" className="w-[18px] h-[18px] object-contain opacity-90" />
        </span>
        <span className="text-xs font-semibold text-text-secondary tracking-normal h-5 flex items-center">
          D2RHub
        </span>
      </div>

      <div className="flex-1" data-tauri-drag-region />

      <div className="flex items-center gap-1.5">
        <button
          onClick={onOpenConfig}
          className="icon-btn w-7 h-7"
          title="配置"
        >
          <Settings size={14} strokeWidth={1.8} />
        </button>
        <button onClick={onStats}
          className="icon-btn w-7 h-7" title="查看统计">
          <BarChart3 size={14} strokeWidth={1.8} />
        </button>
        <button
          onClick={onShareReport}
          className="icon-btn w-7 h-7 disabled:cursor-wait disabled:opacity-40"
          title={sharingReport ? "正在生成战报" : "分享战报（复制图片）"}
          aria-label={sharingReport ? "正在生成战报" : "分享战报并复制图片"}
          aria-busy={sharingReport}
          disabled={sharingReport}
        >
          <Share2 size={14} strokeWidth={1.8} />
        </button>
        <button onClick={onHelp}
          className="icon-btn w-7 h-7" title="帮助文档">
          <BookOpen size={14} strokeWidth={1.8} />
        </button>
        <button onClick={onAbout}
          className="icon-btn w-7 h-7" title="关于">
          <Info size={14} strokeWidth={1.8} />
        </button>
        <button onClick={onExit}
          className="icon-btn w-7 h-7 hover:!text-text-primary" title="最小化到托盘">
          <Minus size={15} strokeWidth={1.8} />
        </button>
      </div>
    </div>
  );
}
