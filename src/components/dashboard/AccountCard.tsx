import React, { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { Play, RotateCw, Sliders, Trash2, FolderOpen, AlertTriangle, X, Locate, Globe2 } from "lucide-react";
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";

import type { AccountMeta, GlobalConfig, LaunchProgress } from "../../store/types";
import { useAccounts } from "../../store/accounts";
import { showToast } from "../ui/Toast";
import { useLaunch } from "../../store/launch";
import { requiresTokenMigration } from "../../utils/regionPaths";

const stepOrder = ["clean", "copy", "launch", "game", "mutex", "connect", "cleanup", "done"];
const stepLabels: Record<string, string> = {
  clean: "清理", copy: "覆盖", launch: "战网", game: "游戏",
  mutex: "互斥", connect: "连接", cleanup: "收尾", done: "完成",
};

interface GridItemProps {
  account: AccountMeta;
  onRename: (id: string, name: string) => void;
  onDelete: (id: string) => void;
  onConfigure: (a: AccountMeta) => void;
  onLaunch: (id: string) => void;
  onBattleNetOnly: (id: string) => void;
  progress?: LaunchProgress | null;
  isMultiSelectMode?: boolean;
  selected?: boolean;
  onToggleSelect?: (id: string) => void;
  onUpdateToken?: (a: AccountMeta) => void;
  config?: GlobalConfig | null;
}

/** Token 有效期: 720 小时（30 天） */
const TOKEN_VALID_HOURS = 720;

function getTokenStatus(lastResetAt: string | null | undefined): {
  expired: boolean;
  warning: boolean;
  remainingHours: number;
  label: string;
} {
  if (!lastResetAt) {
    return { expired: false, warning: false, remainingHours: 0, label: "" };
  }
  const reset = new Date(lastResetAt);
  const now = new Date();
  const elapsedHours = (now.getTime() - reset.getTime()) / (1000 * 60 * 60);
  const remaining = Math.max(0, TOKEN_VALID_HOURS - elapsedHours);

  if (remaining <= 0) {
    return { expired: true, warning: true, remainingHours: 0, label: "过期" };
  }
  if (remaining <= 48) {
    const h = Math.floor(remaining);
    return { expired: false, warning: true, remainingHours: remaining, label: h + "h" };
  }
  const days = Math.floor(remaining / 24);
  return { expired: false, warning: false, remainingHours: remaining, label: days + "d" };
}

function fmtRelative(iso: string): string {
  try {
    const d = new Date(iso), n = new Date();
    const mins = Math.floor((n.getTime() - d.getTime()) / 60000);
    if (mins < 1) return "刚刚";
    if (mins < 60) return `${mins} 分钟前`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `${hrs} 小时前`;
    const days = Math.floor(hrs / 24);
    if (days < 7) return `${days} 天前`;
    return d.toLocaleDateString("zh-CN");
  } catch { return ""; }
}

function getModChipLabel(mod: string): string {
  const tokens = mod.match(/"[^"]*"|'[^']*'|\S+/g) || [];
  const names = tokens
    .map(token => token.replace(/^(['"])(.*)\1$/, "$2"))
    .filter(token => token && !token.startsWith("-"));

  return names.join(" ") || mod;
}

function ProgressInline({ progress }: { progress: NonNullable<LaunchProgress> }) {
  const curIdx = stepOrder.indexOf(progress.step);
  const isActive = progress.status === "running" || progress.status === "ok" || progress.status === "error";
  if (!isActive && progress.step === "done") return null;

  return (
    <div className="flex items-center gap-[3px]">
      {stepOrder.map((step, i) => {
        const done = i < curIdx || (i === curIdx && progress.status === "ok");
        const active = i === curIdx && progress.status === "running";
        const err = i === curIdx && progress.status === "error";
        return (
          <div key={step} className="flex-1" title={stepLabels[step]}>
            <div className={`h-[3px] w-full rounded-full transition-all duration-300 ${
              done ? "bg-success" : active ? "bg-accent progress-pulse-glow" : err ? "bg-error" : "bg-surface-hover"
            }`} />
          </div>
        );
      })}
    </div>
  );
}

function ProgressWrapper({ progress, accountId }: { progress: NonNullable<LaunchProgress>; accountId: string }) {
  const [visible, setVisible] = useState(true);
  const [fadeOut, setFadeOut] = useState(false);

  useEffect(() => {
    const isFinished = progress.step === "done" || progress.status === "error" || (progress.status === "ok" && progress.step === "done");

    if (!isFinished) {
      setVisible(true);
      setFadeOut(false);
      return;
    }

    const fadeTimer = setTimeout(() => {
      setFadeOut(true);
    }, 4500);

    const hideTimer = setTimeout(() => {
      setVisible(false);
      useLaunch.setState((state) => {
        const nextProgress = { ...state.progress };
        delete nextProgress[accountId];
        return { progress: nextProgress };
      });
    }, 5000);

    return () => {
      clearTimeout(fadeTimer);
      clearTimeout(hideTimer);
    };
  }, [progress.step, progress.status, accountId]);

  if (!visible) return null;

  return (
    <div
      className="transition-all duration-500 ease-out"
      style={{
        opacity: fadeOut ? 0 : 1,
        height: fadeOut ? 0 : "auto",
        marginTop: fadeOut ? 0 : "auto",
        marginBottom: fadeOut ? 0 : "auto",
        overflow: "hidden",
      }}
    >
      <ProgressInline progress={progress} />
    </div>
  );
}

export function AccountGridItem({
  account, onRename, onDelete, onConfigure, onLaunch, onBattleNetOnly, progress,
  isMultiSelectMode, selected, onToggleSelect, onUpdateToken, config
}: GridItemProps) {
  const display = account.display_name || account.id;
  const [editingName, setEditingName] = useState(false);
  const [nameDraft, setNameDraft] = useState(display);
  const [confirmDel, setConfirmDel] = useState(false);
  const [modDelConfirmIdx, setModDelConfirmIdx] = useState<number | null>(null);
  const [reinit, setReinit] = useState(false);
  const [modEditing, setModEditing] = useState(false);
  const [modDraft, setModDraft] = useState(account.mod_args || "");
  const { reinitializeAccount, updateAccountMods, markSettingsCustomized } = useAccounts();

  // ── 抽屉配置面板 ──
  const [expanded, setExpanded] = useState(false);
  const [drawerLoaded, setDrawerLoaded] = useState(false);
  type DrawerSettings = { resolution: string; fps: number };
  const [drawer, setDrawer] = useState<DrawerSettings>({ resolution: "1280x720", fps: 30 });
  // 本地窗口位置状态（同步 account prop 和后端）
  const [winX, setWinX] = useState<number | null | undefined>(account.window_x);
  const [winY, setWinY] = useState<number | null | undefined>(account.window_y);

  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingSettingsRef = useRef<Record<string, unknown>>({});

  useEffect(() => { setWinX(account.window_x); setWinY(account.window_y); }, [account.window_x, account.window_y]);
  useEffect(() => { setModDraft(account.mod_args || ""); }, [account.mod_args]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    const setupListener = async () => {
      const stopListening = await listen<{ accountId: string }>("account-settings-updated", (event) => {
        if (event.payload.accountId === account.id) {
          loadDrawerSettings(true);
        }
      });
      if (cancelled) stopListening();
      else unlisten = stopListening;
    };
    void setupListener();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [account.id]);

  const loadDrawerSettings = async (force = false) => {
    if (drawerLoaded && !force) return;
    try {
      const raw = await invoke<Record<string, unknown>>("get_account_settings", { accountId: account.id });
      setDrawer({
        resolution: String(raw["Screen Resolution (Windowed)"] ?? "1280x720"),
        fps: Number(raw["Framerate Target"] ?? raw["Framerate Cap"] ?? 30),
      });
      setDrawerLoaded(true);
    } catch (e) {
      console.warn("Failed to load settings for", account.id, e);
    }
  };

  useEffect(() => {
    if (account.initialized) {
      loadDrawerSettings();
    }
  }, [account.id, account.initialized]);

  const commitMod = () => {
    setModEditing(false);
    const v = modDraft.trim();
    if (!v) return;
    const mods = account.mod_list || [];
    if (!mods.includes(v)) {
      updateAccountMods(account.id, v, [...mods, v]);
    } else if (v !== account.mod_args) {
      updateAccountMods(account.id, v, mods);
    }
  };

  const saveDrawerSetting = (key: string, value: unknown) => {
    setDrawer(prev => ({ ...prev, [key]: value }));

    const configKey = key === "resolution"
      ? "Screen Resolution (Windowed)"
      : "Framerate Target";
    pendingSettingsRef.current[configKey] = value;

    if (saveTimerRef.current) {
      clearTimeout(saveTimerRef.current);
    }

    saveTimerRef.current = setTimeout(async () => {
      try {
        const raw = await invoke<Record<string, unknown>>("get_account_settings", { accountId: account.id });
        const merged = { ...raw, ...pendingSettingsRef.current };
        await invoke("save_account_settings", { accountId: account.id, settings: merged });
        await markSettingsCustomized(account.id);
        pendingSettingsRef.current = {};

        await emit("account-settings-updated", { accountId: account.id });
      } catch (e) {
        showToast("error", `保存设置失败: ${e}`);
      }
    }, 1000);
  };

  const saveWindowPosition = async (x: number | undefined, y: number | undefined) => {
    setWinX(x ?? null);
    setWinY(y ?? null);
    try {
      await invoke("set_account_window_position", { accountId: account.id, windowX: x ?? null, windowY: y ?? null });
    } catch (e) {
      showToast("error", `保存窗口位置失败: ${e}`);
    }
  };

  const handleCardClick = () => {
    if (!account.initialized) return;
    if (!expanded) loadDrawerSettings();
    setExpanded(!expanded);
  };

  const handleReinit = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if ((account.auth_mode === "token" || requiresTokenMigration(account.auth_mode, account.region, config)) && onUpdateToken) {
      onUpdateToken(account);
      return;
    }
    setReinit(true);
    try { await reinitializeAccount(account.id); showToast("success", "已重新初始化"); }
    catch (e) { /* Error is handled and toasted in store */ }
    finally { setReinit(false); }
  };

  const handleDelete = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!confirmDel) { setConfirmDel(true); setTimeout(() => setConfirmDel(false), 4000); return; }
    onDelete(account.id);
  };

  const commitName = () => {
    const v = nameDraft.trim() || account.id;
    if (v !== display) onRename(account.id, v);
    setEditingName(false);
  };

  const handleOpenFolder = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await invoke("open_account_dir", { accountId: account.id });
    } catch (e) {
      showToast("error", `打开文件夹失败: ${e}`);
    }
  };

  const lastLaunchText = account.last_launched_at ? fmtRelative(account.last_launched_at) : null;
  const tokenStatus = account.initialized ? getTokenStatus(account.last_reset_at) : null;
  const regionLabel = account.region === "KR" ? "亚服" : account.region === "NA" ? "美服" : account.region === "EU" ? "欧服" : account.region === "Global" ? "国际服" : "国服";
  const tokenMigrationRequired = requiresTokenMigration(account.auth_mode, account.region, config);
  const performanceLabel = `${drawer.resolution} · ${drawer.fps === 0 ? "unlimited" : `${drawer.fps}fps`}`;
  const configModeLabel = account.has_customized_settings ? "独立配置" : "系统配置";

  // ── 预置选项 ──
  const resOptions = ["1280x720","1600x900","1920x1080","2560x1440","3840x2160"];
  const fpsOptions = [0, 30, 60, 120, 144, 240];
  const stop = (e: React.MouseEvent) => e.stopPropagation();

  return (
    <div
      onClick={handleCardClick}
      className="spatial-tile group account-tile flex min-h-[152px] flex-col animate-card-in"
      data-expanded={expanded ? "true" : "false"}
      data-selected={selected ? "true" : "false"}
      style={{ cursor: account.initialized ? "pointer" : "default" }}
    >
      <div className="tile-core">
        <div className="tile-top">
          <div className="min-w-0">
            <div className="name-row">
              {isMultiSelectMode ? (
                <input
                  type="checkbox"
                  checked={!!selected}
                  disabled={tokenMigrationRequired}
                  onChange={() => onToggleSelect && onToggleSelect(account.id)}
                  onClick={stop}
                  title={tokenMigrationRequired ? "请先迁移为 Token 直启" : undefined}
                  className="h-4 w-4 shrink-0 cursor-pointer rounded border-border-default text-accent accent-accent focus:ring-accent disabled:cursor-not-allowed disabled:opacity-40"
                />
              ) : (
                <span className="tile-index index">{String(account.order + 1).padStart(2, "0")} / {account.initialized ? "READY" : "ATTENTION"}</span>
              )}
            </div>

            <div className="title-mod-row">
              {editingName ? (
                <input
                  className="line-input h-8 min-w-[128px] flex-1 px-2.5 text-base font-semibold"
                  value={nameDraft}
                  onChange={e => setNameDraft(e.target.value)}
                  onKeyDown={e => { if (e.key === "Enter") commitName(); if (e.key === "Escape") { setNameDraft(display); setEditingName(false); } }}
                  onBlur={commitName}
                  onClick={stop}
                  autoFocus
                />
              ) : (
                <button
                  className="tile-name name min-w-0 max-w-full text-left transition-colors duration-200 hover:text-text-secondary"
                  onClick={e => {
                    if (isMultiSelectMode) return;
                    e.stopPropagation();
                    setEditingName(true);
                  }}
                >
                  {display}
                </button>
              )}

              {!isMultiSelectMode && (
                <div className="mod-row inline-mod-row">
                  {(account.mod_list || []).map((mod, idx) => {
                    const isActive = account.mod_args === mod;
                    const isConfirming = modDelConfirmIdx === idx;
                    const modLabel = getModChipLabel(mod);
                    return (
                      <div key={idx} className="group/mod relative flex items-center" onMouseLeave={() => setModDelConfirmIdx(null)}>
                        <button
                          onClick={e => {
                            e.stopPropagation();
                            if (!isActive) {
                              updateAccountMods(account.id, mod, account.mod_list || []);
                            }
                          }}
                          className={`hig-badge mod-chip max-w-[118px] truncate font-mono active:scale-[0.97] ${isActive ? "mod-chip-active" : ""}`}
                          title={isActive ? `${mod}（当前生效）` : `${mod}（点击生效）`}
                        >
                          {modLabel}
                        </button>
                        <button
                          className={`absolute -right-1.5 -top-1.5 z-10 flex h-4 w-4 items-center justify-center rounded-full transition-all ${
                            isConfirming
                              ? "scale-110 text-white opacity-100"
                              : "text-text-muted opacity-0 hover:scale-110 group-hover/mod:opacity-100"
                          }`}
                          style={{ background: isConfirming ? "var(--error)" : "var(--surface-glass)", border: "1px solid var(--border-default)" }}
                          onClick={e => {
                            e.stopPropagation();
                            if (isConfirming) {
                              const newMods = (account.mod_list || []).filter((_, i) => i !== idx);
                              const nextActive = isActive ? (newMods[0] || "") : account.mod_args;
                              updateAccountMods(account.id, nextActive, newMods);
                              setModDelConfirmIdx(null);
                            } else {
                              setModDelConfirmIdx(idx);
                            }
                          }}
                          title={isConfirming ? "确认删除" : "删除配置"}
                        >
                          <X size={9} />
                        </button>
                      </div>
                    );
                  })}
                  {modEditing ? (
                    <input
                      className="line-input h-[24px] w-28 px-2 font-mono text-xs"
                      value={modDraft}
                      onChange={e => setModDraft(e.target.value)}
                      onKeyDown={e => {
                        if (e.key === "Enter") {
                          e.stopPropagation();
                          commitMod();
                        }
                        if (e.key === "Escape") {
                          e.stopPropagation();
                          setModDraft("");
                          setModEditing(false);
                        }
                      }}
                      onBlur={commitMod}
                      onClick={e => e.stopPropagation()}
                      placeholder="-mod xxx"
                      autoFocus
                    />
                  ) : (
                    <button
                      onClick={e => { e.stopPropagation(); setModDraft(""); setModEditing(true); }}
                      className="hig-badge mod-chip w-[24px] justify-center px-0"
                      title="添加 mod"
                    >
                      +
                    </button>
                  )}
                </div>
              )}
            </div>
          </div>
          <span className={account.initialized && !tokenMigrationRequired ? "state-dot state" : "state-dot state warn"} />

        </div>

        <div className="tag-row tag-row-offset">
          <span className="hig-badge hig-badge-neutral">{lastLaunchText || (account.initialized ? "已就绪" : "待配置")}</span>
          <span className="hig-badge hig-badge-neutral">{regionLabel}</span>
          {account.auth_mode === "token" ? (
            <span className="hig-badge hig-badge-violet">网页 Token</span>
          ) : (
            <span className="hig-badge hig-badge-blue">战网认证</span>
          )}
          {tokenMigrationRequired && <span className="hig-badge hig-badge-gold">需迁移 Token</span>}
          {tokenStatus && tokenStatus.label && account.auth_mode !== "token" && !tokenMigrationRequired && (
            <span
              className={`hig-badge ${tokenStatus.expired ? "hig-badge-red" : tokenStatus.warning ? "hig-badge-gold" : "hig-badge-green"}`}
              title={tokenStatus.expired ? "Token 已过期，需重新初始化" : `Token ${tokenStatus.warning ? "即将过期" : "有效"}，剩余 ${tokenStatus.label}`}
            >
              {tokenStatus.expired ? "过期" : tokenStatus.label}
            </span>
          )}
          {account.auth_mode === "token" && <span className="hig-badge hig-badge-green">长期</span>}
          {!account.initialized && <span className="hig-badge hig-badge-red">未初始化</span>}
        </div>

        <div className="tag-row tag-row-secondary tag-row-offset">
          {drawerLoaded && account.initialized && <span className="hig-badge hig-badge-neutral performance-chip">{performanceLabel}</span>}
          {account.initialized && (
            <span className={`hig-badge config-chip ${account.has_customized_settings ? "hig-badge-green" : "hig-badge-neutral"}`}>
              {configModeLabel}
            </span>
          )}
        </div>

        <div className="bottom-row">
          {!isMultiSelectMode && (
            <div className="account-card-actions">
              {tokenMigrationRequired && onUpdateToken ? (
                <button
                  onClick={e => { stop(e); onUpdateToken(account); }}
                  className="primary-cta"
                  title="国际服已停用战网模式，请迁移为 Token 直启"
                >
                  <AlertTriangle size={12} />
                  迁移 Token
                </button>
              ) : account.initialized && (
                <button
                  onClick={e => { stop(e); onLaunch(account.id); }}
                  disabled={account.auth_mode !== "token" && tokenStatus?.expired}
                  className={(account.auth_mode !== "token" && tokenStatus?.expired) ? "danger-cta" : "primary-cta"}
                  title={(account.auth_mode !== "token" && tokenStatus?.expired) ? "Token 已过期，请重新初始化" : undefined}
                >
                  {(account.auth_mode !== "token" && tokenStatus?.expired) ? <AlertTriangle size={12} /> : <Play size={12} />}
                  {(account.auth_mode !== "token" && tokenStatus?.expired) ? "重置" : "启动"}
                </button>
              )}
            <div className="spatial-tools account-card-tools tools">
              {account.initialized && account.auth_mode !== "token" && !tokenMigrationRequired && (
                <button onClick={e => { stop(e); onBattleNetOnly(account.id); }} className="mini-action icon-btn" title="仅启动战网">
                  <Globe2 size={12} strokeWidth={1.8} aria-hidden="true" />
                </button>
              )}
              {account.initialized && (
                <>
                  <button onClick={e => { stop(e); onConfigure(account); }} className="mini-action icon-btn relative" title="高级设置">
                    <Sliders size={12} strokeWidth={1.8} />
                  </button>
                  <button onClick={handleOpenFolder} className="mini-action icon-btn" title="打开配置目录">
                    <FolderOpen size={12} strokeWidth={1.8} />
                  </button>
                  <button onClick={handleReinit} disabled={reinit} className="mini-action icon-btn disabled:opacity-40" title={tokenMigrationRequired ? "迁移为 Token 直启" : "重置"}>
                    <RotateCw size={12} strokeWidth={1.8} className={reinit ? "animate-spin" : ""} />
                  </button>
                </>
              )}
              <button onClick={handleDelete} className={`mini-action icon-btn ${confirmDel ? "!bg-error/10 !text-error" : "hover:!bg-error/10 hover:!text-error"}`} title={confirmDel ? "确认删除" : "删除"}>
                <Trash2 size={12} strokeWidth={1.8} />
              </button>
            </div>
            </div>
          )}
        </div>
      </div>

      {progress ? (
        <div className="mt-0 px-[15px] pb-2">
          <ProgressWrapper progress={progress} accountId={account.id} />
        </div>
      ) : null}

      <div
        className="grid"
        style={{
          gridTemplateRows: expanded ? "1fr" : "0fr",
          transition: "grid-template-rows 0.32s cubic-bezier(0.16, 1, 0.3, 1)",
        }}
      >
        <div style={{ overflow: "hidden", minHeight: 0 }}>
          <div className="drawer-body">
            <div className="drawer-grid">
              <div className="drawer-resolution-fps-row">
                <div className="drawer-field">
                  <label className="micro-meta mb-1.5 block">分辨率</label>
                  <select
                    value={drawer.resolution}
                    onChange={e => saveDrawerSetting("resolution", e.target.value)}
                    onClick={stop}
                    className="line-select w-full px-2.5"
                  >
                    {resOptions.map(r => <option key={r} value={r}>{r}</option>)}
                  </select>
                </div>

                <div className="drawer-field">
                  <label className="micro-meta mb-1.5 block">FPS</label>
                  <div className="combo-input">
                    <input
                      type="number"
                      min={0}
                      max={500}
                      list={`fps-options-${account.id}`}
                      value={drawer.fps}
                      onClick={stop}
                      onChange={e => saveDrawerSetting("fps", Math.max(0, Math.min(500, Number(e.target.value) || 0)))}
                    />
                    <datalist id={`fps-options-${account.id}`}>
                      {fpsOptions.map(f => <option key={f} value={f}>{f === 0 ? "无限制" : `${f} FPS`}</option>)}
                    </datalist>
                  </div>
                </div>
              </div>

              <div>
                <label className="micro-meta mb-1.5 block">坐标</label>
                <div className="drawer-coordinate-controls">
                  <input
                    type="number"
                    placeholder="X"
                    value={winX ?? ""}
                    onClick={stop}
                    onChange={e => {
                      const v = e.target.value;
                      saveWindowPosition(v === "" ? undefined : Number(v), winY ?? undefined);
                    }}
                    className="line-input w-full px-2 text-center"
                  />
                  <input
                    type="number"
                    placeholder="Y"
                    value={winY ?? ""}
                    onClick={stop}
                    onChange={e => {
                      const v = e.target.value;
                      saveWindowPosition(winX ?? undefined, v === "" ? undefined : Number(v));
                    }}
                    className="line-input w-full px-2 text-center"
                  />
                  <button
                    onClick={async (e) => {
                      e.stopPropagation();
                      try {
                        await invoke("move_game_window", { accountId: account.id });
                        showToast("success", "已尝试复位游戏窗口");
                      } catch (err: any) {
                        showToast("error", "复位窗口失败: " + err);
                      }
                    }}
                    className="control-btn shrink-0"
                    title="立即将游戏窗口移动至设定的 X, Y 位置"
                  >
                    <Locate size={11} />
                    复位
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

export function SortableAccountCard({
  account, onRename, onDelete, onConfigure, onLaunch, onBattleNetOnly,
  isMultiSelectMode, selected, onToggleSelect, onUpdateToken, config
}: GridItemProps) {
  const {
    attributes, listeners, setNodeRef, transform, transition, isDragging,
  } = useSortable({ id: account.id, disabled: isMultiSelectMode });
  const { progress } = useLaunch();

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.5 : 1,
    zIndex: isDragging ? 10 : undefined,
    width: "100%",
  };

  return (
    <div ref={setNodeRef} style={style} {...attributes} {...listeners}>
      <AccountGridItem
        account={account}
        onRename={onRename}
        onDelete={onDelete}
        onConfigure={onConfigure}
        onLaunch={onLaunch}
        onBattleNetOnly={onBattleNetOnly}
        progress={progress[account.id] || null}
        isMultiSelectMode={isMultiSelectMode}
        selected={selected}
        onToggleSelect={onToggleSelect}
        onUpdateToken={onUpdateToken}
        config={config}
      />
    </div>
  );
}
