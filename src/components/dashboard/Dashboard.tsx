import React from "react";
import { TopNav } from "./TopNav";
import type { BattleReportQuickRange } from "../../utils/battleReport";

interface DashboardProps {
  onAbout: () => void;
  onExit: () => void;
  onOpenConfig: () => void;
  onHelp: () => void;
  onStats: () => void;
  onShareReport: (range: BattleReportQuickRange) => void;
  sharingReport: boolean;
  children: React.ReactNode;
}

export function Dashboard({
  onAbout, onExit, onOpenConfig, onHelp, onStats, onShareReport, sharingReport, children,
}: DashboardProps) {
  return (
    <div className="flex-1 flex flex-col min-h-0">
      <TopNav
        onAbout={onAbout}
        onExit={onExit}
        onOpenConfig={onOpenConfig}
        onHelp={onHelp}
        onStats={onStats}
        onShareReport={onShareReport}
        sharingReport={sharingReport}
      />
      <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
        {children}
      </div>
    </div>
  );
}
