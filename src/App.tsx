import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AlertTriangle, UserPlus, Zap } from "lucide-react";
import { useGlobalConfig, initConfigListener } from "./store/globalConfig";
import { useAccounts } from "./store/accounts";
import { syncThemeFromConfig } from "./store/theme";
import { useWindowGeometrySave } from "./hooks/useWindowGeometrySave";
import { SetupWizard } from "./pages/SetupWizard";
import { AboutModal } from "./pages/AboutModal";
import { SettingsCenter } from "./components/config/SettingsCenter";
import { ToastContainer, showToast } from "./components/ui/Toast";
import { AppShell } from "./components/layout/AppShell";

import {
  useBongoCatWindow,
  useOverlayWindow,
  useLaunchEvents,
  useAutoUpdate,
  useFirstLaunch,
  usePreventDragRegionDoubleClick,
} from "./hooks/useAppEffects";

import UpdateConfirmModal from "./components/ui/UpdateConfirmModal";
import {
  Dashboard,
  AccountGrid,
  SortableAccountCard,
  AccountGridLoading,
  AccountGridEmpty,
  ActionBar,
  LaunchButton,
} from "./components/dashboard";
import { useLaunch } from "./store/launch";
import { LaunchProgressView } from "./components/Launch/LaunchProgress";
import { AccountInitDialog } from "./components/accounts/AccountInitDialog";
import { requiresTokenMigration } from "./utils/regionPaths";
import { Modal } from "./components/ui/Modal";
import { Button } from "./components/ui/Button";
import type { GlobalConfig, AccountMeta } from "./store/types";
import { setAuxiliaryWindowVisible } from "./utils/windowPlacement";

type View =
  | { type: "loading" }
  | { type: "setup"; existingConfig?: GlobalConfig }
  | { type: "main"; };

function App() {
  const { config, initialLoading, error: configError, load, save } = useGlobalConfig();
  const { loadAccounts, accounts, deleteAccount, renameAccount, reorderAccounts } = useAccounts();
  const { launching, startLaunch, startBattleNetOnly, cancelLaunch, logs, clearLogs } = useLaunch();


  const [view, setView]         = useState<View>({ type: "loading" });
  const [showAbout, setShowAbout] = useState(false);
  const [showInit, setShowInit]   = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [settingsTab, setSettingsTab] = useState<string | null>(null);
  const [settingsAccountId, setSettingsAccountId] = useState<string | null>(null);

  // Kill confirm modal states
  const [showKillConfirm, setShowKillConfirm] = useState(false);
  const [killing, setKilling] = useState(false);

  const handleKillAllD2R = async () => {
    setKilling(true);
    try {
      await invoke("kill_all_d2r_processes");
      showToast("success", "清理完成，所有暗黑2进程已关闭。");
      setShowKillConfirm(false);
    } catch (e) {
      showToast("error", `关闭进程失败: ${e}`);
    } finally {
      setKilling(false);
    }
  };

  // Multi-select modes
  const [isMultiSelectMode, setIsMultiSelectMode] = useState(false);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [tokenUpdateAccount, setTokenUpdateAccount] = useState<AccountMeta | null>(null);

  const toggleMultiSelectMode = () => {
    setIsMultiSelectMode(prev => {
      const next = !prev;
      if (!next) {
        setSelectedIds([]);
      }
      return next;
    });
  };

  const selectAllAccounts = () => {
    const initializedIds = accounts
      .filter(a => a.initialized && !requiresTokenMigration(a.auth_mode, a.region, config))
      .map(a => a.id);
    setSelectedIds(initializedIds);
  };

  const clearSelection = () => {
    setSelectedIds([]);
  };

  const toggleSelectAccount = (id: string) => {
    setSelectedIds(prev =>
      prev.includes(id) ? prev.filter(x => x !== id) : [...prev, id]
    );
  };

  // Auto-update modal states
  const [showAutoUpdateConfirm, setShowAutoUpdateConfirm] = useState(false);
  const [autoUpdateUrl, setAutoUpdateUrl] = useState("");
  const [autoUpdateVersion, setAutoUpdateVersion] = useState("");

  useWindowGeometrySave("save_window_geometry", 100, 100);
  usePreventDragRegionDoubleClick();

  // 等待 DOM 渲染完成后显示窗口（避免白屏闪烁）
  useEffect(() => {
    getCurrentWindow().show().catch(() => {});
    // 应用字体缩放预设
    try {
      const saved = localStorage.getItem("d2rhub-font-scale");
      if (saved && ["small","default","large"].includes(saved)) {
        document.documentElement.dataset.fontScale = saved;
      } else {
        document.documentElement.dataset.fontScale = "default";
      }
    } catch {
      document.documentElement.dataset.fontScale = "default";
    }
  }, []);

  // Load configuration, accounts, and start config listener
  useEffect(() => {
    (async () => { await load(); await loadAccounts(); })();
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    initConfigListener().then(fn => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // Update view state
  useEffect(() => {
    if (initialLoading) return;
    if (!config || !config.first_run_complete) {
      setView({ type: "setup", existingConfig: config ?? undefined });
    }
    else setView({ type: "main" });
    // Sync theme from config (config as source of truth)
    if (config?.theme) syncThemeFromConfig(config.theme);
  }, [initialLoading, config]);

  // Sync font scale from config
  useEffect(() => {
    if (!config?.font_scale) return;
    if (["small","default","large"].includes(config.font_scale)) {
      document.documentElement.dataset.fontScale = config.font_scale;
      try { localStorage.setItem("d2rhub-font-scale", config.font_scale); } catch {}
    }
  }, [config?.font_scale]);

  // Execute App Side Effects
  useBongoCatWindow(initialLoading, config);
  useOverlayWindow(initialLoading, config);
  useLaunchEvents(config);
  useAutoUpdate(initialLoading, config, (version, url) => {
    setAutoUpdateVersion(version);
    setAutoUpdateUrl(url);
    setShowAutoUpdateConfirm(true);
  });
  useFirstLaunch(initialLoading, config, save);

  const handleReconfigure = () => {
    setView({ type: "setup", existingConfig: config ?? undefined });
  };

  if (!initialLoading && configError && !config) return (
    <AppShell>
      <div className="flex-1 flex items-center justify-center px-6">
        <div className="w-[560px] account-line px-6 py-5">
          <div className="flex items-start gap-3">
            <div className="swiss-mark shrink-0">
              <AlertTriangle size={16} className="text-error" strokeWidth={1.8} />
            </div>
            <div className="min-w-0 flex-1">
              <p className="text-sm font-semibold text-text-primary">配置文件无法安全读取</p>
              <p className="text-sm text-text-secondary mt-2 leading-relaxed">
                为避免默认值覆盖现有数据，D2RHub 已停止配置初始化。原文件仍保留；如果存在可用备份，程序会在启动时自动恢复。
              </p>
              <p className="text-xs font-mono text-error mt-3 break-all leading-relaxed">{configError}</p>
              <button type="button" onClick={() => void invoke("open_logs_dir")}
                className="control-btn h-9 mt-4">
                打开日志目录
              </button>
            </div>
          </div>
        </div>
      </div>
    </AppShell>
  );

  // ── loading ──
  if (view.type === "loading") return (
    <AppShell>
      <div className="flex-1 flex items-center justify-center">
        <div className="flex flex-col items-center gap-4">
          <div className="w-10 h-10 rounded-full border-2 border-accent border-t-transparent animate-spin" />
          <p className="text-md text-text-muted">正在加载...</p>
        </div>
      </div>
    </AppShell>
  );

  // ── setup ──
  if (view.type === "setup") return (
    <AppShell>
      <div className="flex-1 flex flex-col">
        <SetupWizard
          initialConfig={view.existingConfig}
          onComplete={() => setView({ type: "main" })}
        />
      </div>
      <ToastContainer />
    </AppShell>
  );

  // ── main ──
  const initialized = accounts.filter(
    a => a.initialized && !requiresTokenMigration(a.auth_mode, a.region, config),
  );
  const sortedAccounts = [...accounts].sort((a, b) => (a.order || 0) - (b.order || 0));

  return (
    <>
      <AppShell>
        <Dashboard
          onAbout={() => setShowAbout(true)}
          onExit={async () => {
            const mainWin = getCurrentWindow();
            await mainWin.hide();
            if (config?.enable_tz_overlay) {
              await setAuxiliaryWindowVisible('overlay', true);
            }
            if (config?.enable_stats_overlay) {
              await setAuxiliaryWindowVisible('stats-overlay', true);
            }
          }}
          onOpenConfig={() => { setShowSettings(true); setSettingsTab(null); setSettingsAccountId(null); }}
          onStats={async () => {
            try {
              await invoke("open_stats_page");
            } catch (e) {
              showToast("error", `打开统计失败: ${e}`);
            }
          }}
          onHelp={async () => {
            try {
              await invoke("open_user_guide");
            } catch (e) {
              showToast("error", `打开帮助文档失败: ${e}`);
            }
          }}
        >
          <ActionBar>
            {launching ? (
              <button
                onClick={cancelLaunch}
                className="danger-cta"
              >
                取消操作
              </button>
            ) : isMultiSelectMode ? (
              <div className="flex items-center gap-2">
                <button
                  disabled={selectedIds.length === 0}
                  onClick={() => startLaunch(selectedIds)}
                  className="primary-cta"
                >
                  <Zap size={13} />
                  启动选中 ({selectedIds.length})
                </button>
                <button
                  onClick={selectAllAccounts}
                  className="control-btn"
                >
                  全选
                </button>
                <button
                  onClick={clearSelection}
                  disabled={selectedIds.length === 0}
                  className="control-btn"
                >
                  清空已选
                </button>
                <button
                  onClick={toggleMultiSelectMode}
                  className="control-btn"
                  data-active="true"
                >
                  退出多选
                </button>
              </div>
            ) : (
              <div className="flex items-center gap-2">
                <LaunchButton
                  count={initialized.length}
                  loading={launching}
                  onClick={() => startLaunch(sortedAccounts
                    .filter(a => a.initialized && !requiresTokenMigration(a.auth_mode, a.region, config))
                    .map(a => a.id))}
                />
                <button
                  onClick={toggleMultiSelectMode}
                  className="control-btn min-w-[72px]"
                  data-active={isMultiSelectMode ? "true" : "false"}
                >
                  多选启动
                </button>
                <button
                  onClick={() => setShowKillConfirm(true)}
                  title="一键关闭所有暗黑2进程"
                  className="control-btn danger-control min-w-[72px]"
                >
                  一键关闭
                </button>
              </div>
            )}
            <div className="flex-1" />
            <div className="flex items-center gap-2">
              {!isMultiSelectMode && (
                <button
                  onClick={() => setShowInit(true)}
                  className="control-btn add-account-cta"
                >
                  <UserPlus size={13} strokeWidth={1.9} />
                  添加账号
                </button>
              )}
            </div>
          </ActionBar>

          {initialLoading ? (
            <AccountGridLoading />
          ) : accounts.length === 0 ? (
            <AccountGridEmpty onAddAccount={() => setShowInit(true)} />
          ) : (
            <>
              <AccountGrid accounts={sortedAccounts} onReorder={reorderAccounts} isMultiSelectMode={isMultiSelectMode}>
                {sortedAccounts.map(a => (
                  <SortableAccountCard
                    key={a.id}
                    account={a}
                    onRename={renameAccount}
                    onDelete={deleteAccount}
                    onConfigure={a => { setShowSettings(true); setSettingsTab("accounts"); setSettingsAccountId(a.id); }}
                    onLaunch={id => startLaunch([id])}
                    onBattleNetOnly={id => startBattleNetOnly([id])}
                    isMultiSelectMode={isMultiSelectMode}
                    selected={selectedIds.includes(a.id)}
                    onToggleSelect={toggleSelectAccount}
                    onUpdateToken={setTokenUpdateAccount}
                    config={config}
                  />
                ))}
              </AccountGrid>

              <LaunchProgressView
                accounts={accounts}
                logs={logs}
                onClear={clearLogs}
              />
            </>
          )}
        </Dashboard>
      </AppShell>

      <AccountInitDialog
        open={showInit || !!tokenUpdateAccount}
        onClose={() => { setShowInit(false); setTokenUpdateAccount(null); }}
        onDone={() => { setShowInit(false); setTokenUpdateAccount(null); }}
        updateAccount={tokenUpdateAccount}
      />
      <AboutModal open={showAbout} onClose={() => setShowAbout(false)} />
      <Modal
        open={showKillConfirm}
        onClose={() => setShowKillConfirm(false)}
        title="确认关闭进程"
        footer={
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setShowKillConfirm(false)}>
              取消
            </Button>
            <Button variant="danger" onClick={handleKillAllD2R} loading={killing}>
              确认关闭
            </Button>
          </div>
        }
      >
        <div className="text-sm text-text-secondary py-2">
          确定要强制关闭当前系统内运行的所有暗黑破坏神II：重制版（D2R.exe）进程吗？此操作可能会导致未保存的游戏进度丢失。
        </div>
      </Modal>
      <SettingsCenter
        open={showSettings}
        onClose={() => setShowSettings(false)}
        onReconfigure={handleReconfigure}
        initialTab={settingsTab}
        initialAccountId={settingsAccountId}
      />

      <UpdateConfirmModal
        open={showAutoUpdateConfirm}
        onClose={() => setShowAutoUpdateConfirm(false)}
        version={autoUpdateVersion}
        downloadUrl={autoUpdateUrl}
      />

      <ToastContainer />
    </>
  );
}

export default App;
