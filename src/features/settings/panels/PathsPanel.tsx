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
              ? "游戏目录可支撑核心启动；战网认证还需客户端路径，存档目录仅供画质覆盖。"
              : "亚/美/欧服共用，仅支持 Token 直启；存档目录仅供画质覆盖。"}
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
          <label className="text-xs text-text-muted block">国服战网客户端 (Battle.net.exe)</label>
          <div className="flex gap-2">
            <input aria-label="国服战网客户端 (Battle.net.exe)" type="text" value={config.cn_battle_net_path} readOnly className="flex-1 h-8 px-3 rounded-lg bg-surface-hover text-xs border border-border-default text-text-primary" />
            <Button size="sm" onClick={() => pickFile("cn_battle_net_path", "国服 Battle.net.exe", ["exe"])}>浏览</Button>
          </div>
        </>
      )}

      <label className="text-xs text-text-muted block">游戏安装目录</label>
      <div className="flex gap-2">
        <input aria-label={`${label}游戏安装目录`} type="text" value={config[fields.game]} readOnly className="flex-1 h-8 px-3 rounded-lg bg-surface-hover text-xs border border-border-default text-text-primary" />
        <Button size="sm" onClick={() => pickFolder(fields.game, `${label}游戏安装目录`)}>浏览</Button>
      </div>

      <label className="text-xs text-text-muted block">
        存档目录（可选） · Diablo II Resurrected{isCn ? " (CN)" : ""}
      </label>
      <div className="flex gap-2">
        <input aria-label={`${label}存档目录`} type="text" value={config[fields.savedGames]} readOnly className="flex-1 h-8 px-3 rounded-lg bg-surface-hover text-xs border border-border-default text-text-primary" />
        <Button size="sm" onClick={() => pickFolder(fields.savedGames, `${label}存档目录`)}>浏览</Button>
        <Button size="sm" onClick={() => applyDetectedPath(fields.savedGames, detectedSavedGames)}>自动探测</Button>
      </div>

      {settingsAvailable === false && (
        <div className="flex items-start gap-2 px-3 py-2.5 rounded-lg" style={{ background: "var(--toast-warning-bg)", border: "1px solid var(--toast-warning-border)" }}>
          <ShieldAlert size={14} className="text-warning shrink-0 mt-0.5" />
          <p className="text-xs text-text-secondary leading-relaxed">
            {label}存档目录中未检测到 Settings.json，账号独立画质快照与覆盖暂不可用。
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
  return (
    <div className="settings-content-grid">
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

      <div className="spatial-panel p-3 space-y-2">
        <h3 className="text-xs font-bold text-text-primary">国服战网/浏览器辅助路径</h3>
        {([
          ["program_data_agent_path", "战网进程 Agent.exe 目录", "Agent 目录", "agent"],
          ["app_data_roaming_bnet_path", "战网 Roaming AppData 目录", "Roaming 战网目录", "roaming"],
        ] as const).map(([field, label, dialogTitle, detectedKey]) => (
          <div className="space-y-2" key={field}>
            <label className="text-xs text-text-muted block">{label}</label>
            <div className="flex gap-2">
              <input aria-label={label} type="text" value={config[field]} readOnly className="flex-1 h-8 px-3 rounded-lg bg-surface-hover text-xs border border-border-default text-text-primary" />
              <Button size="sm" onClick={() => pickFolder(field, dialogTitle)}>浏览</Button>
              <Button size="sm" onClick={() => applyDetectedPath(field, detectedPaths[detectedKey])}>自动探测</Button>
            </div>
          </div>
        ))}

        <div className="space-y-2">
          <label className="text-xs text-text-muted block">独立隔离浏览器程序 (Edge/Chrome.exe)</label>
          <div className="flex gap-2">
            <input aria-label="独立隔离浏览器程序 (Edge/Chrome.exe)" type="text" value={config.browser_path} readOnly className="flex-1 h-8 px-3 rounded-lg bg-surface-hover text-xs border border-border-default text-text-primary" />
            <Button size="sm" onClick={() => pickFile("browser_path", "chrome.exe/msedge.exe", ["exe"])}>浏览</Button>
            <Button size="sm" onClick={() => applyDetectedPath("browser_path", detectedPaths.browser)}>自动探测</Button>
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
      </div>
    </div>
  );
}
