import { type Dispatch, type SetStateAction } from "react";
import { Input } from "../../../components/ui/Input";
import { ResolutionInput } from "../../../components/ui/ResolutionInput";
import { CaptureWindowPositionButton } from "../../windowLayouts/CaptureWindowPositionButton";
import {
  AudioSection,
  AutomapSection,
  DisplaySection,
  GameplaySection,
  GraphicsSection,
  type SettingsMap,
} from "../gameSettings/fields";
import type { AccountMeta } from "../../../store/types";
import { useGlobalConfig } from "../../../store/globalConfig";
import { FRAMERATE_CAP_KEY, readFramerateCap } from "../../../utils/gameSettings";
import "./corePanels.css";

export type GameSettingsTab =
  | "launch"
  | "game_display"
  | "game_graphics"
  | "game_audio"
  | "game_gameplay"
  | "game_automap";

interface AccountsPanelProps {
  accounts: AccountMeta[];
  selectedAccountId: string;
  selectedAccount?: AccountMeta;
  setSelectedAccountId: (accountId: string) => void;
  accountHasChanges: boolean;
  saveAccount: (quiet?: boolean) => Promise<boolean>;
  toggleCustomizedSettings: (accountId: string, customized: boolean) => Promise<void>;
  accountRegionLabel: (region?: string | null) => string;
  gameSettingsTab: GameSettingsTab;
  setGameSettingsTab: Dispatch<SetStateAction<GameSettingsTab>>;
  snapshotSystemSettings: () => Promise<void>;
  accountNicknameDraft: string;
  setAccountNicknameDraft: Dispatch<SetStateAction<string>>;
  onOpenModManager: () => void;
  accountWinXDraft: number | null;
  setAccountWinXDraft: Dispatch<SetStateAction<number | null>>;
  accountWinYDraft: number | null;
  setAccountWinYDraft: Dispatch<SetStateAction<number | null>>;
  gameSettings: SettingsMap;
  gameSettingsLoading: boolean;
  gameSettingsLoadError: string | null;
  updateGameSetting: (key: string, value: unknown) => void;
  loadGameSettings: (accountId: string) => Promise<void>;
}

const GAME_SETTINGS_TABS: ReadonlyArray<{ id: GameSettingsTab; label: string; enLabel: string }> = [
  { id: "launch", label: "启动", enLabel: "Launch" },
  { id: "game_display", label: "显示", enLabel: "Display" },
  { id: "game_graphics", label: "图形", enLabel: "Graphics" },
  { id: "game_audio", label: "音频", enLabel: "Audio" },
  { id: "game_gameplay", label: "玩法", enLabel: "Gameplay" },
  { id: "game_automap", label: "地图", enLabel: "Map" },
];

function describeGameSettingsLoadError(error: string, en: boolean): { title: string; detail: string } {
  if (error.includes("正在执行另一项操作")) {
    return {
      title: en ? "Account settings are in use" : "账号配置正在使用中",
      detail: en ? `${error}. Reload after the current operation finishes.` : `${error}。当前操作完成后重新读取。`,
    };
  }
  if (error.includes("正在修改共享 Battle.net 环境")) {
    return {
      title: en ? "Launch is preparing account settings" : "启动流程正在准备配置",
      detail: en ? `${error}. Reload after the account has launched.` : `${error}。账号启动完成后重新读取。`,
    };
  }
  if (error.includes("Settings.json") && (error.includes("不存在") || error.includes("未检测到"))) {
    return {
      title: en ? "Game settings not found" : "尚未找到画质配置",
      detail: en ? `${error}. Enter the game once with this client, then return to capture an account snapshot.` : `${error}。请先使用对应客户端进入游戏一次，再返回创建账号快照。`,
    };
  }
  if (error.includes("序列化/反序列化") || error.includes("无效") || error.includes("为空")) {
    return {
      title: en ? "Invalid game settings format" : "画质配置格式无效",
      detail: en ? `${error}. Let the game recreate Settings.json, or restore a valid copy.` : `${error}。请让游戏重新生成 Settings.json，或恢复一份有效配置。`,
    };
  }
  return {
    title: en ? "Could not read game settings" : "未能读取画质配置",
    detail: en ? `${error}. Reload to retry. If this continues, open the logs in Maintenance.` : `${error}。请稍后重新读取；若持续失败，可在维护工具中打开运行日志。`,
  };
}

export function AccountsPanel({
  accounts,
  selectedAccountId,
  selectedAccount,
  setSelectedAccountId,
  accountHasChanges,
  saveAccount: handleSaveAccount,
  toggleCustomizedSettings: handleToggleAccountSettingsMode,
  accountRegionLabel,
  gameSettingsTab,
  setGameSettingsTab,
  snapshotSystemSettings: handleSnapshotSystemSettings,
  accountNicknameDraft,
  setAccountNicknameDraft,
  onOpenModManager,
  accountWinXDraft,
  setAccountWinXDraft,
  accountWinYDraft,
  setAccountWinYDraft,
  gameSettings,
  gameSettingsLoading,
  gameSettingsLoadError,
  updateGameSetting,
  loadGameSettings,
}: AccountsPanelProps) {
  const en = useGlobalConfig(state => state.config?.app_language === "en-US");
  const copy = en ? {
    empty: "Choose Add account on the dashboard to create an account first.", select: "Select account", override: "Override game settings",
    overrideHint: "Use this account's saved game settings at launch", token: "Web Token", bnet: "Battle.net", uninitialized: "Not initialized",
    title: "Graphics & launch", description: "Manage account details, launch options, and in-game settings here.", categories: "Account settings categories",
    snapshot: "Capture system settings", nickname: "Nickname", currentMod: "Current Mod", vanilla: "Original game", mods: "Manage Mods",
    windowX: "Window X", windowY: "Window Y", default: "Default", resolution: "Resolution", unlimited: "Unlimited",
    positionHint: "Saved as a new account position preset", loading: "Loading game settings. Resolution and FPS will be available shortly…", reload: "Reload settings",
  } : {
    empty: "请先在主界面点击“添加账号”创建新账号", select: "选择账号", override: "覆盖游戏配置",
    overrideHint: "启动时使用此账号保存的游戏配置", token: "网页 Token", bnet: "战网认证", uninitialized: "未初始化",
    title: "画质与启动", description: "账号字段、启动参数和游戏内配置在这里完成。", categories: "账号设置分类",
    snapshot: "快照系统配置", nickname: "昵称", currentMod: "当前 Mod", vanilla: "原版游戏", mods: "Mod 管理",
    windowX: "窗口 X", windowY: "窗口 Y", default: "默认", resolution: "分辨率", unlimited: "无限制",
    positionHint: "保存为新的账号位置配置", loading: "正在读取游戏配置，分辨率和帧数稍后可调整…", reload: "重新读取",
  };
  const gameSubTabs = GAME_SETTINGS_TABS;
  const gameSettingsError = gameSettingsLoadError
    ? describeGameSettingsLoadError(gameSettingsLoadError, en)
    : null;
  return (
<div className="accounts-settings-panel space-y-3 p-1">
  {accounts.length === 0 ? (
    <div className="spatial-panel py-10 text-center text-sm text-text-muted" data-i18n-skip>{copy.empty}</div>
  ) : (
    <div className="grid grid-cols-[230px_minmax(0,1fr)] max-[880px]:grid-cols-1 gap-4">
      <div className="space-y-2 max-[880px]:flex max-[880px]:gap-2 max-[880px]:space-y-0 max-[880px]:overflow-x-auto">
        {accounts.map((a, idx) => {
          const active = a.id === selectedAccountId;
          const overrideEnabled = !!a.has_customized_settings;
          return (
            <div
              key={a.id}
              className="account-option w-full text-left"
              data-selected={active ? "true" : "false"}
            >
              <button
                type="button"
                aria-label={`${copy.select} ${a.display_name || a.id}`}
                aria-pressed={active}
                onClick={async () => {
                  if (a.id === selectedAccountId) return;
                  if (accountHasChanges) {
                    if (!(await handleSaveAccount(true))) return;
                  }
                  setSelectedAccountId(a.id);
                }}
                className="absolute inset-0 z-0 rounded-[inherit] focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-2"
              />
              <div className="account-option-top pointer-events-none relative z-[1]">
                <div className="min-w-0">
                  <span className="tile-index">{String(idx + 1).padStart(2, "0")}</span>
                  <div className="account-option-name truncate">{a.display_name || a.id}</div>
                </div>
                <span className={`${a.initialized ? "account-state-dot" : "account-state-dot warn"} absolute right-0 top-0`} aria-hidden="true" />
              </div>
              <div className="account-option-meta pointer-events-none relative z-[1]">
                <span className="hig-badge hig-badge-neutral">{en ? ({ CN: "China", KR: "Asia", NA: "Americas", EU: "Europe", Global: "Global" } as Record<string, string>)[a.region || "CN"] || a.region : accountRegionLabel(a.region)}</span>
                <span className={a.auth_mode === "token" ? "hig-badge hig-badge-violet" : "hig-badge hig-badge-blue"}>
                  {a.auth_mode === "token" ? copy.token : copy.bnet}
                </span>
                {!a.initialized && <span className="hig-badge hig-badge-red">{copy.uninitialized}</span>}
              </div>
              <label className="account-option-custom option-line pointer-events-auto relative z-[1]" title={copy.overrideHint}>
                <input type="checkbox" className="sr-only" checked={overrideEnabled}
                  aria-label={`${a.display_name || a.id}：${copy.override}`}
                  onChange={e => void handleToggleAccountSettingsMode(a.id, e.target.checked)} />
                <span className={overrideEnabled ? "check-box checked" : "check-box"} />
                <span>{copy.override}</span>
              </label>
            </div>
          );
        })}
      </div>

      {selectedAccountId && selectedAccount && (
        <div className="setting-card min-h-[340px]">
          <div className="mb-3 border-b border-border-default pb-3" data-i18n-skip>
            <div className="min-w-0">
              <p className="text-sm font-bold text-text-primary">{selectedAccount.display_name || selectedAccount.id} · {copy.title}</p>
              <p className="micro-meta mt-1">{copy.description}</p>
            </div>
            <div className="mt-3 flex flex-wrap items-center justify-between gap-2">
              <div className="settings-subnav" role="tablist" aria-label={copy.categories}>
                {gameSubTabs.map((tab, index) => {
                  const active = gameSettingsTab === tab.id;
                  return (
                  <button
                    key={tab.id}
                    id={`account-settings-tab-${tab.id}`}
                    type="button"
                    role="tab"
                    aria-selected={active}
                    aria-controls="account-settings-panel"
                    tabIndex={active ? 0 : -1}
                    onClick={() => setGameSettingsTab(tab.id)}
                    onKeyDown={event => {
                      if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
                      event.preventDefault();
                      const nextIndex = event.key === "Home"
                        ? 0
                        : event.key === "End"
                          ? gameSubTabs.length - 1
                          : event.key === "ArrowRight"
                            ? (index + 1) % gameSubTabs.length
                            : (index - 1 + gameSubTabs.length) % gameSubTabs.length;
                      const next = gameSubTabs[nextIndex];
                      setGameSettingsTab(next.id);
                      window.requestAnimationFrame(() => {
                        document.getElementById(`account-settings-tab-${next.id}`)?.focus();
                      });
                    }}
                    className="control-btn h-8 px-3"
                    data-active={active ? "true" : "false"}
                  >
                    {en ? tab.enLabel : tab.label}
                  </button>
                  );
                })}
              </div>
              <button
                type="button"
                onClick={handleSnapshotSystemSettings}
                disabled={gameSettingsLoading}
                className="control-btn h-8 shrink-0 px-3"
              >
                {copy.snapshot}
              </button>
            </div>
          </div>

          <div
            id="account-settings-panel"
            role="tabpanel"
            aria-labelledby={`account-settings-tab-${gameSettingsTab}`}
          >
          {gameSettingsTab === "launch" && (
            <div className="space-y-3" data-i18n-skip>
              <div className="grid grid-cols-2 gap-3 max-[720px]:grid-cols-1">
                <Input
                  label={copy.nickname}
                  value={accountNicknameDraft}
                  onChange={e => setAccountNicknameDraft(e.target.value)}
                />
                <div className="account-mod-readonly">
                  <span>{copy.currentMod}</span>
                  <div><code>{selectedAccount?.mod_args || copy.vanilla}</code><button type="button" className="control-btn" onClick={onOpenModManager}>{copy.mods}</button></div>
                </div>
              </div>

              <div className="grid grid-cols-2 gap-3 max-[720px]:grid-cols-1">
                <Input
                  label={copy.windowX}
                  type="number"
                  value={accountWinXDraft !== null ? accountWinXDraft : ""}
                  onChange={e => setAccountWinXDraft(e.target.value !== "" ? Number(e.target.value) : null)}
                  placeholder={copy.default}
                />
                <Input
                  label={copy.windowY}
                  type="number"
                  value={accountWinYDraft !== null ? accountWinYDraft : ""}
                  onChange={e => setAccountWinYDraft(e.target.value !== "" ? Number(e.target.value) : null)}
                  placeholder={copy.default}
                />
              </div>

              <fieldset disabled={gameSettingsLoading || !!gameSettingsLoadError} className="min-w-0 grid grid-cols-2 gap-3 max-[720px]:grid-cols-1">
                <div>
                  <label className="micro-meta mb-1.5 block">{copy.resolution}</label>
                  <ResolutionInput
                    label={copy.resolution}
                    value={String(gameSettings["Screen Resolution (Windowed)"] ?? "1280x720")}
                    onChange={value => updateGameSetting("Screen Resolution (Windowed)", value)}
                  />
                </div>
                <div>
                  <label className="micro-meta mb-1.5 block">FPS</label>
                  <div className="combo-input">
                    <input
                      aria-label="FPS"
                      type="number"
                      min={0}
                      max={500}
                      list="settings-center-fps-options"
                      value={readFramerateCap(gameSettings, 60)}
                      onChange={e => updateGameSetting(FRAMERATE_CAP_KEY, Math.max(0, Math.min(500, Number(e.target.value) || 0)))}
                    />
                    <datalist id="settings-center-fps-options">
                      {[0, 30, 60, 120, 144, 240].map(f => <option key={f} value={f}>{f === 0 ? copy.unlimited : `${f} FPS`}</option>)}
                    </datalist>
                  </div>
                </div>
              </fieldset>
              <div className="flex flex-wrap items-center gap-2">
                <CaptureWindowPositionButton accountId={selectedAccountId} english={en}
                  beforeCapture={() => accountHasChanges ? handleSaveAccount(true) : Promise.resolve(true)}
                  onCaptured={account => { setAccountWinXDraft(account.window_x ?? null); setAccountWinYDraft(account.window_y ?? null); }} />
                <span className="text-xs text-text-secondary">{copy.positionHint}</span>
              </div>
            </div>
          )}

          {gameSettingsTab !== "launch" && gameSettingsLoading && (
            <div className="space-y-3 py-4">
              {[1, 2, 3].map(i => (
                <div key={i} className="h-9 skeleton rounded-lg" />
              ))}
            </div>
          )}

          {gameSettingsTab === "launch" && gameSettingsLoading && <p className="text-xs text-text-muted py-2" role="status" data-i18n-skip>{copy.loading}</p>}
          {!gameSettingsLoading && gameSettingsError && (
            <div className="rounded-lg border border-warning/40 bg-warning/5 p-4" data-i18n-skip>
              <p className="text-sm font-semibold text-text-primary">{gameSettingsError.title}</p>
              <p className="mt-1 text-xs leading-relaxed text-text-secondary">
                {gameSettingsError.detail}
              </p>
              <button
                type="button"
                className="control-btn mt-3 h-8 px-3"
                onClick={() => void loadGameSettings(selectedAccountId)}
              >
                {copy.reload}
              </button>
            </div>
          )}

          {gameSettingsTab !== "launch" && !gameSettingsLoading && !gameSettingsLoadError && (
            <div className="space-y-4">
              {gameSettingsTab === "game_display" && (
                <DisplaySection settings={gameSettings} update={updateGameSetting} />
              )}
              {gameSettingsTab === "game_graphics" && (
                <GraphicsSection settings={gameSettings} update={updateGameSetting} />
              )}
              {gameSettingsTab === "game_audio" && (
                <AudioSection settings={gameSettings} update={updateGameSetting} />
              )}
              {gameSettingsTab === "game_gameplay" && (
                <GameplaySection settings={gameSettings} update={updateGameSetting} />
              )}
              {gameSettingsTab === "game_automap" && (
                <AutomapSection settings={gameSettings} update={updateGameSetting} />
              )}
            </div>
          )}
          </div>
        </div>
      )}
    </div>
  )}
</div>
  );
}
