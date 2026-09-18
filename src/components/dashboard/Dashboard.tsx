import React from "react";
import { TopNav } from "./TopNav";
import type { BattleReportQuickRange } from "../../utils/battleReport";

interface DashboardProps {
  onAbout: () => void;
  onExit: () => void;
  onOpenConfig: () => void;
  onHelp: () => void;
  onStats: () => void;
  statsModuleInstalled?: boolean;
  onShareReport: (range: BattleReportQuickRange) => void;
  sharingReport: boolean;
  children: React.ReactNode;
  onMiniMode?: () => void;
  miniModeDisabled?: boolean;
}

export function Dashboard({
  onAbout, onExit, onOpenConfig, onHelp, onStats, statsModuleInstalled = false,
  onShareReport, sharingReport, children, onMiniMode, miniModeDisabled,
}: DashboardProps) {
  return (
    <div className="flex-1 flex flex-col min-h-0">
      <TopNav
        onMiniMode={onMiniMode}
        miniModeDisabled={miniModeDisabled}
        onAbout={onAbout}
        onExit={onExit}
        onOpenConfig={onOpenConfig}
        onHelp={onHelp}
        onStats={onStats}
        statsModuleInstalled={statsModuleInstalled}
        onShareReport={onShareReport}
        sharingReport={sharingReport}
      />
      <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
        {children}
      </div>
    </div>
  );
}
