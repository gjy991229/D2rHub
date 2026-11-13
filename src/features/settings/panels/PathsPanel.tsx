import "./corePanels.css";
import { ShieldAlert } from "lucide-react";

import type { GlobalConfig } from "../../../store/types";
import { Button } from "../../../components/ui/Button";

type InstallationPathField =
  | "cn_battle_net_path"
  | "cn_game_path"
  | "cn_saved_games_path"
  | "global_game_path"
  | "global_saved_games_path";

interface PathsPanelProps {
  config: GlobalConfig;
  settingsAvailable: Record<"CN" | "Global", boolean | null>;
  detectedPaths: Record<string, string | null>;
  updateConfig: (updater: (config: GlobalConfig) => void) => void;
  pickFile: (field: keyof GlobalConfig, title: string, extensions?: string[]) => Promise<void>;
  pickFolder: (field: keyof GlobalConfig, title: string) => Promise<void>;
  applyDetectedPath: (field: keyof GlobalConfig, value: string | null) => void;
}

interface InstallationProfileFieldsProps extends Pick<
  PathsPanelProps,
  "config" | "updateConfig" | "pickFile" | "pickFolder" | "applyDetectedPath"
> {
  edition: "CN" | "Global";
  settingsAvailable: boolean | null;
  detectedSavedGames: string | null;
}

function InstallationProfileFields({
  edition,
  config,
  settingsAvailable,
  detectedSavedGames,
  updateConfig,
  pickFile,
  pickFolder,
  applyDetectedPath,
}: InstallationProfileFieldsProps) {
  const isCn = edition === "CN";
  const en = config.app_language === "en-US";
  const label = isCn ? "国服" : "国际服";
  const fields: Record<"game" | "savedGames", InstallationPathField> = isCn
    ? { game: "cn_game_path", savedGames: "cn_saved_games_path" }
    : { game: "global_game_path", savedGames: "global_saved_games_path" };
  const hasConfiguration = Object.values(fields).some(field => config[field])
    || (isCn && Boolean(config.cn_battle_net_path));
  const profileComplete = Boolean(config[fields.game].trim());

  const clearProfile = () => {
    updateConfig(next => {
      if (isCn) next.cn_battle_net_path = "";
      next[fields.game] = "";
      next[fields.savedGames] = "";
    });
  };

  return (
    <div className="space-y-2 border-t border-border-default/50 pt-3">
      <div className="flex items-center justify-between gap-3">
        <div>
          <p className="text-xs font-semibold text-text-secondary">{label}</p>
          <p className="text-2xs text-text-muted mt-0.5">
            {isCn
              ? "支持战网或 Token；配置目录仅用于账号独立画质。"
              : "亚/美/欧服共用，仅 Token；配置目录仅用于账号独立画质。"}
          </p>
        </div>
        {hasConfiguration && <Button size="sm" className="shrink-0 whitespace-nowrap" onClick={clearProfile}>清除此版本</Button>}
      </div>

      {hasConfiguration && !profileComplete && (
        <p className="text-xs text-text-muted leading-relaxed">
          当前版本尚未配置游戏安装目录；不会开放该版本的账号创建与启动。
        </p>
      )}

      {isCn && (
        <>
          <label htmlFor="path-cn-bnet" className="text-xs text-text-muted block">国服战网客户端 (Battle.net.exe)</label>
          <div className="path-input-row">
            <input id="path-cn-bnet" aria-label="国服战网客户端 (Battle.net.exe)" title={config.cn_battle_net_path} placeholder={en ? "Choose Battle.net.exe" : "最终选择 Battle.net.exe"} type="text" value={config.cn_battle_net_path} readOnly className="min-w-0 flex-1 h-9 px-3 rounded-lg bg-surface-hover text-xs border border-border-default text-text-primary" />
            <Button aria-label="浏览国服战网客户端" size="sm" className="shrink-0 whitespace-nowrap" onClick={() => pickFile("cn_battle_net_path", "国服 Battle.net.exe", ["exe"])}>浏览</Button>
          </div>
          <p className="text-2xs text-text-muted">最终应选择：Battle.net.exe；前缀路径不限。</p>
        </>
      )}

      <label htmlFor={`path-${fields.game}`} className="text-xs text-text-muted block">游戏安装目录</label>
      <div className="path-input-row">
        <input id={`path-${fields.game}`} aria-label={`${label}游戏安装目录`} title={config[fields.game]} placeholder={en ? "Choose Diablo II Resurrected (containing D2R.exe)" : "最终选择 游戏文件夹（包含 D2R.exe）"} type="text" value={config[fields.game]} readOnly className="min-w-0 flex-1 h-9 px-3 rounded-lg bg-surface-hover text-xs border border-border-default text-text-primary" />
        <Button aria-label={`浏览${label}游戏安装目录`} size="sm" className="shrink-0 whitespace-nowrap" onClick={() => pickFolder(fields.game, `${label}游戏安装目录`)}>浏览</Button>
      </div>

      <p className="text-2xs text-text-muted">最终应选择：含 D2R.exe 的文件夹，文件夹名称和路径不限。</p>

      <label htmlFor={`path-${fields.savedGames}`} className="text-xs text-text-muted block">
        配置目录（可选） · Diablo II Resurrected{isCn ? " (CN)" : ""}
      </label>
      <div className="path-input-row">
        <input id={`path-${fields.savedGames}`} aria-label={`${label}配置目录`} title={config[fields.savedGames]} placeholder={`最终选择 Diablo II Resurrected${isCn ? " (CN)" : ""}`} type="text" value={config[fields.savedGames]} readOnly className="min-w-0 flex-1 h-9 px-3 rounded-lg bg-surface-hover text-xs border border-border-default text-text-primary" />
        <Button aria-label={`浏览${label}配置目录`} size="sm" className="shrink-0 whitespace-nowrap" onClick={() => pickFolder(fields.savedGames, `${label}配置目录`)}>浏览</Button>
        <Button aria-label={`使用探测到的${label}配置目录`} title={detectedSavedGames || (en ? "No default folder found. Browse to choose one." : "未探测到默认目录，请浏览选择")} disabled={!detectedSavedGames} size="sm" className="shrink-0 whitespace-nowrap" onClick={() => applyDetectedPath(fields.savedGames, detectedSavedGames)}>探测</Button>
      </div>



      <p className="text-2xs text-text-muted">最终应选择：Diablo II Resurrected{isCn ? " (CN)" : ""} 文件夹；前缀路径不限，不要求 Settings.json。</p>

      {settingsAvailable === false && (
        <div className="flex items-start gap-2 px-3 py-2.5 rounded-lg" style={{ background: "var(--toast-warning-bg)", border: "1px solid var(--toast-warning-border)" }}>
          <ShieldAlert size={14} className="text-warning shrink-0 mt-0.5" />
          <p className="text-xs text-text-secondary leading-relaxed">
            {label}配置目录中未检测到 Settings.json，账号独立画质快照与覆盖暂不可用。
          </p>
        </div>
      )}
    </div>
  );
}

export function PathsPanel({
  config,
  settingsAvailable,
  detectedPaths,
  updateConfig,
  pickFile,
  pickFolder,
  applyDetectedPath,
}: PathsPanelProps) {
  const en = config.app_language === "en-US";
  return (
    <div className="settings-content-grid settings-paths-grid">
      <div className="spatial-panel p-3 space-y-2">
        <h3 className="text-xs font-bold text-text-primary">核心程序路径</h3>
        <InstallationProfileFields
          edition="CN"
          config={config}
          settingsAvailable={settingsAvailable.CN}
          detectedSavedGames={detectedPaths.cnSavedGames}
          updateConfig={updateConfig}
          pickFile={pickFile}
          pickFolder={pickFolder}
          applyDetectedPath={applyDetectedPath}
        />
        <InstallationProfileFields
          edition="Global"
          config={config}
          settingsAvailable={settingsAvailable.Global}
          detectedSavedGames={detectedPaths.globalSavedGames}
          updateConfig={updateConfig}
          pickFile={pickFile}
          pickFolder={pickFolder}
          applyDetectedPath={applyDetectedPath}
        />
      </div>

      <details className="spatial-panel p-3 space-y-2">
        <summary className="settings-advanced-summary">辅助路径与浏览器 <span>自动探测不正确时再调整</span></summary>
        {([
          ["program_data_agent_path", "Agent.exe 目录", "Agent 目录", "agent"],
          ["app_data_roaming_bnet_path", "Battle.net 数据目录", "Roaming 战网目录", "roaming"],
        ] as const).map(([field, label, dialogTitle, detectedKey]) => (
          <div className="space-y-2" key={field}>
            <label htmlFor={`path-${field}`} className="text-xs text-text-muted block">{label}</label>
            <div className="path-input-row">
              <input id={`path-${field}`} aria-label={label} title={config[field]} placeholder={en ? "Detect automatically or browse" : "自动探测，或浏览选择"} type="text" value={config[field]} readOnly className="min-w-0 flex-1 h-9 px-3 rounded-lg bg-surface-hover text-xs border border-border-default text-text-primary" />
              <Button aria-label={`浏览${label}`} size="sm" className="shrink-0 whitespace-nowrap" onClick={() => pickFolder(field, dialogTitle)}>浏览</Button>
              <Button aria-label={`使用探测到的${label}`} title={detectedPaths[detectedKey] || (en ? "No default folder found. Browse to choose one." : "未探测到默认目录，请浏览选择")} disabled={!detectedPaths[detectedKey]} size="sm" className="shrink-0 whitespace-nowrap" onClick={() => applyDetectedPath(field, detectedPaths[detectedKey])}>探测</Button>
            </div>
          </div>
        ))}

        <div className="space-y-2">
          <label htmlFor="path-browser" className="text-xs text-text-muted block">隔离浏览器 (Edge/Chrome.exe)</label>
          <div className="path-input-row">
            <input id="path-browser" aria-label="独立隔离浏览器程序 (Edge/Chrome.exe)" title={config.browser_path} placeholder={en ? "Choose msedge.exe or chrome.exe" : "选择 msedge.exe 或 chrome.exe"} type="text" value={config.browser_path} readOnly className="min-w-0 flex-1 h-9 px-3 rounded-lg bg-surface-hover text-xs border border-border-default text-text-primary" />
            <Button aria-label="浏览隔离浏览器程序" size="sm" className="shrink-0 whitespace-nowrap" onClick={() => pickFile("browser_path", "chrome.exe/msedge.exe", ["exe"])}>浏览</Button>
            <Button aria-label="使用探测到的隔离浏览器" title={detectedPaths.browser || (en ? "No browser found. Browse to choose one." : "未探测到浏览器，请浏览选择")} disabled={!detectedPaths.browser} size="sm" className="shrink-0 whitespace-nowrap" onClick={() => applyDetectedPath("browser_path", detectedPaths.browser)}>探测</Button>
          </div>
        </div>

        <div className="flex items-center justify-between pt-1">
          <div>
            <span className="text-sm font-semibold text-text-secondary">隔离浏览器类型</span>
            <p className="text-2xs text-text-muted">当前选择的沙箱浏览器品牌</p>
          </div>
          <select
            aria-label="隔离浏览器类型"
            value={config.browser_type}
            onChange={event => updateConfig(next => { next.browser_type = event.target.value; })}
            className="h-[28px] px-2.5 rounded-lg bg-surface-hover border border-border-default text-text-primary text-xs"
          >
            <option value="">未指定</option>
            <option value="chrome">Google Chrome</option>
            <option value="edge">Microsoft Edge</option>
          </select>
        </div>
      </details>
    </div>
  );
}
