import type { Dispatch, SetStateAction } from "react";
import { Input } from "../../../components/ui/Input";
import {
  AudioSection,
  AutomapSection,
  DisplaySection,
  GameplaySection,
  GraphicsSection,
  type SettingsMap,
} from "../../../pages/SettingsEditor";
import type { AccountMeta } from "../../../store/types";
import { FRAMERATE_CAP_KEY, readFramerateCap } from "../../../utils/gameSettings";

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

const GAME_SETTINGS_TABS: ReadonlyArray<{ id: GameSettingsTab; label: string }> = [
  { id: "launch", label: "启动" },
  { id: "game_display", label: "显示" },
  { id: "game_graphics", label: "图形" },
  { id: "game_audio", label: "音频" },
  { id: "game_gameplay", label: "玩法" },
  { id: "game_automap", label: "地图" },
];

function describeGameSettingsLoadError(error: string): { title: string; detail: string } {
  if (error.includes("正在执行另一项操作")) {
    return {
      title: "账号配置正在使用中",
      detail: `${error}。当前操作完成后重新读取。`,
    };
  }
  if (error.includes("正在修改共享 Battle.net 环境")) {
    return {
      title: "启动流程正在准备配置",
      detail: `${error}。账号启动完成后重新读取。`,
    };
  }
  if (error.includes("Settings.json") && (error.includes("不存在") || error.includes("未检测到"))) {
    return {
      title: "尚未找到画质配置",
      detail: `${error}。请先使用对应客户端进入游戏一次，再返回创建账号快照。`,
    };
  }
  if (error.includes("序列化/反序列化") || error.includes("无效") || error.includes("为空")) {
    return {
      title: "画质配置格式无效",
      detail: `${error}。请让游戏重新生成 Settings.json，或恢复一份有效配置。`,
    };
  }
  return {
    title: "未能读取画质配置",
    detail: `${error}。请稍后重新读取；若持续失败，可在维护工具中打开运行日志。`,
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
  const gameSubTabs = GAME_SETTINGS_TABS;
  const gameSettingsError = gameSettingsLoadError
    ? describeGameSettingsLoadError(gameSettingsLoadError)
    : null;
  return (
<div className="space-y-3">
  {accounts.length === 0 ? (
    <div className="spatial-panel py-10 text-center text-sm text-text-muted">请先在主界面点击“添加账号”创建新账号</div>
  ) : (
    <div className="grid grid-cols-[248px_minmax(0,1fr)] gap-3 max-[880px]:grid-cols-1">
      <div className="space-y-1.5 max-[880px]:order-1 max-[880px]:flex max-[880px]:gap-2 max-[880px]:space-y-0 max-[880px]:overflow-x-auto max-[880px]:pb-1">
        {accounts.map((a, idx) => {
          const active = a.id === selectedAccountId;
          const overrideEnabled = !!a.has_customized_settings;
          return (
            <div
              key={a.id}
              className="account-option w-full text-left max-[880px]:w-[230px] max-[880px]:shrink-0"
              data-selected={active ? "true" : "false"}
            >
              <button
                type="button"
                aria-label={`选择账号 ${a.display_name || a.id}`}
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
                <div className="flex shrink-0 items-start gap-2">
                  <label
                    className="option-line option-line-inline pointer-events-auto"
                    title="覆盖游戏配置"
                  >
                    <input
                      type="checkbox"
                      className="sr-only"
                      checked={overrideEnabled}
                      onChange={e => void handleToggleAccountSettingsMode(a.id, e.target.checked)}
                    />
                    <span className={overrideEnabled ? "check-box checked" : "check-box"} />
                    <span>覆盖游戏配置</span>
                  </label>
                  <span className={a.initialized ? "account-state-dot" : "account-state-dot warn"} />
                </div>
              </div>
              <div className="account-option-meta pointer-events-none relative z-[1]">
                <span className="hig-badge hig-badge-neutral">{accountRegionLabel(a.region)}</span>
                <span className={a.auth_mode === "token" ? "hig-badge hig-badge-violet" : "hig-badge hig-badge-blue"}>
                  {a.auth_mode === "token" ? "网页 Token" : "战网认证"}
                </span>
                {!a.initialized && <span className="hig-badge hig-badge-red">未初始化</span>}
              </div>
            </div>
          );
        })}
      </div>

      {selectedAccountId && selectedAccount && (
        <div className="setting-card min-h-[340px] max-[880px]:order-2">
          <div className="mb-3 border-b border-border-default pb-3">
            <div className="min-w-0">
              <p className="text-sm font-bold text-text-primary">{selectedAccount.display_name || selectedAccount.id} · 画质与启动</p>
              <p className="micro-meta mt-1">账号字段、启动参数和游戏内配置在这里完成。</p>
            </div>
            <div className="mt-3 flex flex-wrap items-center justify-between gap-2">
              <div className="settings-subnav" role="tablist" aria-label="账号设置分类">
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
                    {tab.label}
                  </button>
                  );
                })}
              </div>
              <button
                type="button"
                onClick={handleSnapshotSystemSettings}
                className="control-btn h-8 shrink-0 px-3"
              >
                快照系统配置
              </button>
            </div>
          </div>

          <div
            id="account-settings-panel"
            role="tabpanel"
            aria-labelledby={`account-settings-tab-${gameSettingsTab}`}
          >
          {gameSettingsTab === "launch" && (
            <div className="space-y-3">
              <div className="grid grid-cols-2 gap-3 max-[720px]:grid-cols-1">
                <Input
                  label="昵称"
                  value={accountNicknameDraft}
                  onChange={e => setAccountNicknameDraft(e.target.value)}
                />
                <div className="account-mod-readonly">
                  <span>当前 Mod</span>
                  <div><code>{selectedAccount?.mod_args || "原版游戏"}</code><button type="button" className="control-btn" onClick={onOpenModManager}>Mod 管理</button></div>
                </div>
              </div>

              <div className="grid grid-cols-2 gap-3 max-[720px]:grid-cols-1">
                <Input
                  label="窗口 X"
                  type="number"
                  value={accountWinXDraft !== null ? accountWinXDraft : ""}
                  onChange={e => setAccountWinXDraft(e.target.value !== "" ? Number(e.target.value) : null)}
                  placeholder="默认"
                />
                <Input
                  label="窗口 Y"
                  type="number"
                  value={accountWinYDraft !== null ? accountWinYDraft : ""}
                  onChange={e => setAccountWinYDraft(e.target.value !== "" ? Number(e.target.value) : null)}
                  placeholder="默认"
                />
              </div>

              <div className="grid grid-cols-2 gap-3 max-[720px]:grid-cols-1">
                <div>
                  <label className="micro-meta mb-1.5 block">分辨率</label>
                  <select
                    aria-label="分辨率"
                    value={String(gameSettings["Screen Resolution (Windowed)"] ?? "1280x720")}
                    onChange={e => updateGameSetting("Screen Resolution (Windowed)", e.target.value)}
                    className="line-select w-full px-2.5"
                  >
                    {["1280x720","1600x900","1920x1080","2560x1440","3840x2160"].map(r => <option key={r} value={r}>{r}</option>)}
                  </select>
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
                      {[0, 30, 60, 120, 144, 240].map(f => <option key={f} value={f}>{f === 0 ? "无限制" : `${f} FPS`}</option>)}
                    </datalist>
                  </div>
                </div>
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

          {gameSettingsTab !== "launch" && !gameSettingsLoading && gameSettingsError && (
            <div className="rounded-lg border border-warning/40 bg-warning/5 p-4">
              <p className="text-sm font-semibold text-text-primary">{gameSettingsError.title}</p>
              <p className="mt-1 text-xs leading-relaxed text-text-secondary">
                {gameSettingsError.detail}
              </p>
              <button
                type="button"
                className="control-btn mt-3 h-8 px-3"
                onClick={() => void loadGameSettings(selectedAccountId)}
              >
                重新读取
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
