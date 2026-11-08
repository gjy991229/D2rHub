import React, { useState, useEffect, useRef } from "react";
import { invokeCommand } from "../../platform/tauri";
import {
  AlertTriangle,
  ChevronDown,
  MoreHorizontal,
  FolderOpen,
  Globe2,
  Locate,
  Play,
  RotateCw,
  Sliders,
  Trash2,
  X,
} from "lucide-react";
import { AnchoredPanel } from "../ui/AnchoredPanel";
import { Modal } from "../ui/Modal";
import { Button } from "../ui/Button";
import { ResolutionInput } from "../ui/ResolutionInput";
import { CaptureWindowPositionButton } from "../../features/windowLayouts/CaptureWindowPositionButton";

import type {
  AccountMeta,
  GlobalConfig,
  LaunchGroupMember,
  LaunchProgress,
  ModCapsulePool,
  WindowPositionPreset,
} from "../../store/types";
import { useAccounts } from "../../store/accounts";
import { showToast } from "../ui/Toast";
import { useLaunch } from "../../store/launch";
import {
  accountRegionLabel,
  isInternationalRegion,
  requiresTokenMigration,
} from "../../utils/regionPaths";
import { AccountRegionSwitcher } from "./AccountRegionSwitcher";
import { AccountModEditor } from "./AccountModEditor";
import { AccountStatusLight, type AccountStatusLightProps } from "./AccountStatusLight";
import {
  type AccountQuickSettings,
  useAccountQuickSettings,
} from "../../hooks/useAccountQuickSettings";

const stepOrder = ["clean", "copy", "launch", "game", "mutex", "connect", "cleanup", "done"];
const stepLabels: Record<string, string> = {
  clean: "清理", copy: "覆盖", launch: "战网", game: "游戏",
  mutex: "互斥", connect: "连接", cleanup: "收尾", done: "完成",
};

export interface GridItemProps {
  runtimeStatus?: Omit<AccountStatusLightProps, "name" | "running">;
  account: AccountMeta;
  onRename: (id: string, name: string) => Promise<boolean>;
  onDelete: (id: string) => void;
  onConfigure: (a: AccountMeta) => void;
  onLaunch: (id: string) => void;
  onBattleNetOnly: (id: string) => void;
  progress?: LaunchProgress | null;
  isSelectionMode?: boolean;
  selected?: boolean;
  onToggleSelect?: (id: string) => void;
  schemeMember?: LaunchGroupMember;
  onSchemeMemberChange?: (id: string, patch: Partial<LaunchGroupMember>) => void;
  modCapsulePool?: ModCapsulePool | null;
  modCapsuleLoading?: boolean;
  modCapsuleError?: string | null;
  onRequestModCapsules?: () => Promise<unknown>;
  modCapsuleAssigningAccountId?: string | null;
  onAssignModCapsule?: (accountId: string, capsuleId: string | null) => Promise<unknown>;
  onOpenModManager?: (action?: "add", edition?: string | null) => void;
  getPositionSchemeUsage?: (id: string, positionId: string) => string[];
  onUpdateToken?: (a: AccountMeta) => void;
  onReinitialize?: (a: AccountMeta) => void;
  config?: GlobalConfig | null;
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

function createPositionId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `position-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function ProgressInline({ progress }: { progress: NonNullable<LaunchProgress> }) {
  const curIdx = stepOrder.indexOf(progress.step);
  const isActive = progress.status === "running" || progress.status === "ok" || progress.status === "error";
  if (!isActive && progress.step === "done") return null;

  return (
    <div className="flex items-center gap-[3px]" role="progressbar" aria-label={progress.message}
      aria-valuemin={0} aria-valuemax={stepOrder.length} aria-valuenow={Math.max(0, curIdx + (progress.status === "ok" ? 1 : 0))}
      aria-valuetext={progress.message} title={progress.message}>
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
  isSelectionMode, selected, onToggleSelect, schemeMember, onSchemeMemberChange,
  modCapsulePool, modCapsuleAssigningAccountId, onAssignModCapsule, onOpenModManager,
  modCapsuleLoading, modCapsuleError, onRequestModCapsules,
  onUpdateToken, onReinitialize, config, runtimeStatus,
}: GridItemProps) {
  const display = account.display_name || account.id;
  const [editingName, setEditingName] = useState(false);
  const [nameDraft, setNameDraft] = useState(display);
  const [confirmDel, setConfirmDel] = useState(false);
  const [reinit, setReinit] = useState(false);
  const [positionEditing, setPositionEditing] = useState(false);
  const [positionNameDraft, setPositionNameDraft] = useState("");
  const [positionXDraft, setPositionXDraft] = useState("0");
  const [positionYDraft, setPositionYDraft] = useState("0");
  const [positionDelConfirmId, setPositionDelConfirmId] = useState<string | null>(null);
  const {
    reinitializeAccount,
    updateAccountPositions,
  } = useAccounts();

  // ── 抽屉配置面板 ──
  const [expanded, setExpanded] = useState(false);
  const [moreOpen, setMoreOpen] = useState(false);
  const [quickClosing, setQuickClosing] = useState(false);
  const quickAnchor = useRef<HTMLButtonElement>(null);
  const moreAnchor = useRef<HTMLButtonElement>(null);
  const deleteCancel = useRef<HTMLButtonElement>(null);
  const positionAdd = useRef<HTMLButtonElement>(null);
  const cancelledRename = useRef(false);
  const closingQuick = useRef<Promise<boolean> | null>(null);
  const english = config?.app_language === "en-US";
  const quickSettingsEnabled = account.initialized
    && (expanded || (Boolean(isSelectionMode) && Boolean(selected)));
  const {
    settings: drawer,
    loaded: drawerLoaded,
    loading: drawerLoading,
    error: drawerLoadError,
    load: loadDrawerSettings,
    update: updateDrawerSettings,
    flush: flushDrawerSettings,
  } = useAccountQuickSettings(account.id, quickSettingsEnabled);
  const nameCommitInFlightRef = useRef(false);

  useEffect(() => {
    if (!isSelectionMode || !selected || !schemeMember || !drawerLoaded
      || schemeMember.graphics_configured) return;
    onSchemeMemberChange?.(account.id, {
      graphics_configured: true,
      resolution: drawer.resolution,
      fps: drawer.fps,
    });
  }, [
    account.id,
    drawer.fps,
    drawer.resolution,
    drawerLoaded,
    isSelectionMode,
    onSchemeMemberChange,
    schemeMember,
    selected,
  ]);

  const positionPresets = account.position_presets || [];
  const selectedPositionId = isSelectionMode
    ? schemeMember?.position_preset_id ?? null
    : account.active_position_id ?? null;

  const selectPosition = (positionId: string | null) => {
    if (isSelectionMode) {
      onSchemeMemberChange?.(account.id, {
        position_preset_id: positionId,
        position_configured: true,
      });
      return;
    }
    void updateAccountPositions(account.id, positionId, positionPresets);
  };

  const selectDrawerSetting = (key: keyof AccountQuickSettings, value: string | number) => {
    if (isSelectionMode) {
      onSchemeMemberChange?.(account.id, {
        graphics_configured: true,
        [key]: value,
      });
      return;
    }
    updateDrawerSettings({ [key]: value });
  };

  const leavePositionEditor = () => {
    setPositionEditing(false);
    positionAdd.current?.focus({ preventScroll: true });
  };

  const commitPosition = async () => {
    const name = positionNameDraft.trim();
    const x = Number(positionXDraft);
    const y = Number(positionYDraft);
    if (!name) {
      showToast("warning", "请输入位置名称");
      return;
    }
    if (!Number.isInteger(x) || !Number.isInteger(y)) {
      showToast("warning", "X、Y 坐标必须是整数");
      return;
    }
    if (positionPresets.some(position => position.name.trim().toLocaleLowerCase() === name.toLocaleLowerCase())) {
      showToast("warning", `位置名称“${name}”已存在`);
      return;
    }
    const preset: WindowPositionPreset = { id: createPositionId(), name, x, y };
    const saved = await updateAccountPositions(
      account.id,
      isSelectionMode ? account.active_position_id ?? null : preset.id,
      [...positionPresets, preset],
    );
    if (!saved) return;
    if (isSelectionMode) {
      onSchemeMemberChange?.(account.id, {
        position_preset_id: preset.id,
        position_configured: true,
      });
    }
    leavePositionEditor();
    setPositionNameDraft("");
  };

  const deletePosition = async (position: WindowPositionPreset) => {
    const next = positionPresets.filter(candidate => candidate.id !== position.id);
    const nextActive = account.active_position_id === position.id
      ? next[0]?.id ?? null
      : account.active_position_id ?? null;
    const saved = await updateAccountPositions(account.id, nextActive, next);
    if (saved && isSelectionMode && selectedPositionId === position.id) {
      onSchemeMemberChange?.(account.id, {
        position_preset_id: null,
        position_configured: true,
      });
    }
    setPositionDelConfirmId(null);
  };

  const handleCardClick = () => {
    if (isSelectionMode) {
      const canToggle = !!selected || (account.initialized && !tokenMigrationRequired);
      if (canToggle) onToggleSelect?.(account.id);
      return;
    }
    if (!account.initialized) return;
    if (!expanded) void loadDrawerSettings().catch(() => undefined);
    setExpanded(!expanded);
  };

  const closeQuick = () => {
    if (closingQuick.current) return closingQuick.current;
    setQuickClosing(true);
    const attempt = (async () => {
      try {
        await flushDrawerSettings();
        setExpanded(false);
        return true;
      } catch {
        // Do not hide unsaved changes; the hook retains the patch and error.
        return false;
      }
    })();
    closingQuick.current = attempt;
    void attempt.finally(() => { closingQuick.current = null; setQuickClosing(false); });
    return attempt;
  };

  const handleReinit = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if ((account.auth_mode === "token" || requiresTokenMigration(account.auth_mode, account.region, config)) && onUpdateToken) {
      onUpdateToken(account);
      return;
    }
    if (onReinitialize) {
      onReinitialize(account);
      return;
    }
    setReinit(true);
    try { await reinitializeAccount(account.id); showToast("success", "已重新初始化"); }
    catch (e) { /* Error is handled and toasted in store */ }
    finally { setReinit(false); }
  };

  const handleDelete = (e: React.MouseEvent) => {
    e.stopPropagation();
    setMoreOpen(false);
    setConfirmDel(true);
  };

  const commitName = async () => {
    if (cancelledRename.current) { cancelledRename.current = false; return; }
    if (nameCommitInFlightRef.current) return;
    const v = nameDraft.trim() || account.id;
    setEditingName(false);
    if (v === display) return;

    nameCommitInFlightRef.current = true;
    try {
      const renamed = await onRename(account.id, v);
      if (!renamed) setNameDraft(display);
    } finally {
      nameCommitInFlightRef.current = false;
    }
  };

  const handleOpenFolder = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await invokeCommand("open_account_dir", { accountId: account.id });
    } catch (e) {
      showToast("error", `打开文件夹失败: ${e}`);
    }
  };

  const lastLaunchText = account.last_launched_at ? fmtRelative(account.last_launched_at) : null;
  const regionLabel = accountRegionLabel(account.region);
  const tokenMigrationRequired = requiresTokenMigration(account.auth_mode, account.region, config);
  const canSwitchInternationalRegion = account.auth_mode === "token"
    && isInternationalRegion(account.region);
  const effectiveResolution = isSelectionMode
    ? schemeMember?.resolution ?? drawer.resolution
    : drawer.resolution;
  const effectiveFps = isSelectionMode
    ? schemeMember?.fps ?? drawer.fps
    : drawer.fps;
  const configModeLabel = isSelectionMode
    ? "方案画质"
    : account.has_customized_settings ? "独立配置" : "系统配置";
  const drawerExpanded = isSelectionMode ? !!selected : expanded;

  // ── 预置选项 ──
  const fpsOptions = [0, 30, 60, 120, 144, 240];
  const stop = (e: React.MouseEvent) => e.stopPropagation();
  // “重置”按钮同时是未完成账号的补全入口：待完成 Token 账号在这里补 Token，
  // 尚未跑完首次初始化的战网账号在这里重新进入初始化事务。
  const reinitializeTitle = tokenMigrationRequired
    ? "迁移为 Token 直启"
    : account.initialized
      ? "重置"
      : account.auth_mode === "token"
        ? "补全 Token 完成初始化"
        : "初始化账号";

  const quickFields = (
      <div
        className="grid"
        inert={!drawerExpanded}
        aria-hidden={!drawerExpanded}
        style={{
          gridTemplateRows: drawerExpanded ? "1fr" : "0fr",
          transition: "grid-template-rows 180ms cubic-bezier(0.2, 0.8, 0.2, 1)",
        }}
      >
        <div style={{ overflow: "hidden", minHeight: 0 }}>
          <div className="drawer-body">
            <div className="drawer-grid">
              {drawerLoadError && (
                <div className="scheme-settings-error hig-badge hig-badge-red" title={drawerLoadError}>
                  {drawerLoadError.includes("正在执行另一项操作")
                    ? "账号配置正忙，请稍后重试"
                    : `画质配置${drawerLoaded ? "保存" : "读取"}失败`}
                </div>
              )}
              <div className="drawer-resolution-fps-row">
                <div className="drawer-field">
                  <label className="micro-meta mb-1.5 block">分辨率</label>
                  <ResolutionInput
                    label={`${display} ${english ? "resolution" : "分辨率"}`}
                    value={effectiveResolution}
                    onChange={value => selectDrawerSetting("resolution", value)}
                    onCommit={() => void flushDrawerSettings().catch(() => undefined)}
                    onClick={stop}
                    disabled={drawerLoading}
                  />
                </div>

                <div className="drawer-field">
                  <label className="micro-meta mb-1.5 block">FPS</label>
                  <div className="combo-input">
                    <input
                      type="number"
                      aria-label={`${display} FPS`}
                      min={0}
                      max={500}
                      list={`fps-options-${account.id}`}
                      value={effectiveFps}
                      onClick={stop}
                      onChange={e => selectDrawerSetting("fps", Math.max(0, Math.min(500, Number(e.target.value) || 0)))}
                      onBlur={() => void flushDrawerSettings().catch(() => undefined)}
                      disabled={drawerLoading}
                    />
                    <datalist id={`fps-options-${account.id}`}>
                      {fpsOptions.map(f => <option key={f} value={f}>{f === 0 ? "无限制" : `${f} FPS`}</option>)}
                    </datalist>
                  </div>
                </div>
              </div>

              <div className={isSelectionMode ? "scheme-position-field" : undefined}>
                <label className="micro-meta mb-1.5 block">位置</label>
                <div className="position-preset-row" role="group" aria-label={`${display} 窗口位置`}>
                  <CaptureWindowPositionButton accountId={account.id} activate={!isSelectionMode} english={english}
                    onCaptured={captured => {
                      const preset = captured.position_presets?.[captured.position_presets.length - 1];
                      if (isSelectionMode && preset) onSchemeMemberChange?.(account.id, { position_preset_id: preset.id, position_configured: true });
                    }} />
                  <button
                    type="button"
                    aria-pressed={selectedPositionId === null}
                    className={`hig-badge mod-chip position-chip ${selectedPositionId === null ? "mod-chip-active" : ""}`}
                    onClick={event => { event.stopPropagation(); selectPosition(null); }}
                    title="启动时不调整窗口位置"
                  >
                    不指定
                  </button>
                  {positionPresets.map(position => {
                    const active = selectedPositionId === position.id;
                    const confirming = positionDelConfirmId === position.id;
                    return (
                      <div
                        key={position.id}
                        className="group/position relative flex items-center"
                        onMouseLeave={() => setPositionDelConfirmId(null)}
                      >
                        <button
                          type="button"
                          aria-pressed={active}
                          className={`hig-badge mod-chip position-chip active:scale-[0.97] ${active ? "mod-chip-active" : ""}`}
                          onClick={event => { event.stopPropagation(); selectPosition(position.id); }}
                          title={`${position.name}（${position.x}, ${position.y}）`}
                        >
                          <span>{position.name}</span>
                          <span className="position-chip-coordinates">{position.x},{position.y}</span>
                        </button>
                        <button
                          type="button"
                          className={`position-chip-delete ${confirming ? "is-confirming" : ""}`}
                          aria-label={confirming ? `确认删除位置“${position.name}”` : `删除位置“${position.name}”`}
                          title={confirming ? "再次点击确认删除" : "删除位置"}
                          onClick={event => {
                            event.stopPropagation();
                            if (confirming) void deletePosition(position);
                            else setPositionDelConfirmId(position.id);
                          }}
                        >
                          <X size={9} aria-hidden="true" />
                        </button>
                      </div>
                    );
                  })}
                  <button
                    type="button"
                    onClick={event => {
                      event.stopPropagation();
                      setPositionNameDraft("");
                      setPositionXDraft(String(account.window_x ?? 0));
                      setPositionYDraft(String(account.window_y ?? 0));
                      setPositionEditing(true);
                    }}
                    className="hig-badge mod-chip position-add-chip"
                    ref={positionAdd}
                    title="添加位置"
                  >
                    +
                  </button>
                  {!isSelectionMode && <button
                    onClick={async (e) => {
                      e.stopPropagation();
                      try {
                        await invokeCommand("move_game_window", { accountId: account.id });
                        showToast("success", "已尝试复位游戏窗口");
                      } catch (err: any) {
                        showToast("error", "复位窗口失败: " + err);
                      }
                    }}
                    className="control-btn position-locate-button shrink-0"
                    title="立即将游戏窗口移动到当前默认位置"
                    disabled={selectedPositionId === null}
                  >
                    <Locate size={11} />
                    复位
                  </button>}
                </div>
                {positionEditing && (
                  <div className="position-preset-editor" onClick={stop} onKeyDown={event => {
                    if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); leavePositionEditor(); }
                  }}>
                    <label>
                      <span>名称</span>
                      <input
                        className="line-input px-2"
                        value={positionNameDraft}
                        maxLength={16}
                        placeholder="例如：左上"
                        onChange={event => setPositionNameDraft(event.target.value)}
                        autoFocus
                      />
                    </label>
                    <label>
                      <span>X</span>
                      <input
                        type="number"
                        className="line-input px-2 text-center"
                        value={positionXDraft}
                        onChange={event => setPositionXDraft(event.target.value)}
                      />
                    </label>
                    <label>
                      <span>Y</span>
                      <input
                        type="number"
                        className="line-input px-2 text-center"
                        value={positionYDraft}
                        onChange={event => setPositionYDraft(event.target.value)}
                        onKeyDown={event => {
                          if (event.key === "Enter") void commitPosition();
                        }}
                      />
                    </label>
                    <button type="button" className="primary-cta" onClick={() => void commitPosition()}>
                      保存位置
                    </button>
                    <button type="button" className="control-btn" onClick={leavePositionEditor}>
                      取消
                    </button>
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>
  );

  return (
    <div
      onClick={isSelectionMode ? handleCardClick : undefined}
      className="spatial-tile group account-tile flex min-h-[152px] flex-col animate-card-in"
      data-expanded={expanded ? "true" : "false"}
      data-selected={selected ? "true" : "false"}
      data-batch-selected={runtimeStatus?.selected || undefined}
      data-scheme-edit={isSelectionMode ? "true" : undefined}
      style={{
        cursor: isSelectionMode
          ? (selected || (account.initialized && !tokenMigrationRequired) ? "pointer" : "not-allowed")
          : "default",
      }}
    >
      <div className="tile-core">
        <div className="tile-top">
          <div className="min-w-0">
            <div className="name-row">
              {isSelectionMode ? (
                <>
                  <input
                    type="checkbox"
                    aria-label={`将 ${display} 加入启动方案`}
                    checked={!!selected}
                    disabled={!selected && (!account.initialized || tokenMigrationRequired)}
                    onChange={() => onToggleSelect && onToggleSelect(account.id)}
                    onClick={stop}
                    title={!account.initialized
                      ? "请先初始化账号"
                      : tokenMigrationRequired
                        ? "请先迁移为 Token 直启"
                        : undefined}
                    className="h-4 w-4 shrink-0 cursor-pointer rounded border-border-default text-accent accent-accent focus:ring-accent disabled:cursor-not-allowed disabled:opacity-40"
                  />

                </>
              ) : null}
            </div>

            <div className="title-mod-row">
              {editingName ? (
                <input
                  aria-label={english ? `Rename ${display}` : `重命名 ${display}`}
                  className="line-input h-8 min-w-[128px] flex-1 px-2.5 text-base font-semibold"
                  value={nameDraft}
                  onChange={e => setNameDraft(e.target.value)}
                  onKeyDown={e => { if (e.key === "Enter") void commitName(); if (e.key === "Escape") { cancelledRename.current = true; setNameDraft(display); setEditingName(false); } }}
                  onBlur={() => { void commitName(); }}
                  onClick={stop}
                  autoFocus
                />
              ) : (
                <button
                  title={`${display}${lastLaunchText ? `\n最近启动：${lastLaunchText}` : ""}`}
                  className="tile-name name min-w-0 max-w-full text-left transition-colors duration-200 hover:text-text-secondary"
                  onClick={e => {
                    if (isSelectionMode) return;
                    e.stopPropagation();
                    cancelledRename.current = false;
                    setEditingName(true);
                  }}
                >
                  <span data-i18n-skip>{display}</span>
                </button>
              )}


            </div>
          </div>
          <AccountStatusLight
            name={display} running={account.is_running}
            issue={!account.initialized ? "账号尚未初始化" : tokenMigrationRequired ? "请先迁移为 Token 直启" : null}
            {...runtimeStatus}
            english={english}
            activity={runtimeStatus?.activity || (progress?.status === "running" ? "启动中" : undefined)}
            disabled={isSelectionMode || runtimeStatus?.disabled}
            onRepair={() => {
              if ((!account.initialized && account.auth_mode === "token") || tokenMigrationRequired) onUpdateToken?.(account);
              else if (!account.initialized) onReinitialize?.(account);
              else onConfigure(account);
            }}
          />

        </div>

        <div className="tag-row tag-row-offset">
              {(!isSelectionMode || selected) && (
                <AccountModEditor
                  account={account}
                  isSelectionMode={isSelectionMode}
                  schemeMember={schemeMember}
                  onSchemeMemberChange={onSchemeMemberChange}
                  modCapsulePool={modCapsulePool}
                  poolLoading={modCapsuleLoading}
                  poolError={modCapsuleError}
                  onRequestPool={onRequestModCapsules}
                  assigning={modCapsuleAssigningAccountId === account.id}
                  onAssign={onAssignModCapsule}
                  onOpenModManager={onOpenModManager}
                />
              )}
          {canSwitchInternationalRegion && !isSelectionMode ? (
            <AccountRegionSwitcher
              accountId={account.id}
              currentRegion={account.region}
              isRunning={account.is_running}
            />
          ) : (
            <span className="hig-badge hig-badge-neutral">{regionLabel}</span>
          )}
          {account.auth_mode === "token" ? (
            <span className="hig-badge hig-badge-violet">网页 Token</span>
          ) : (
            <span className="hig-badge hig-badge-blue">战网认证</span>
          )}
          {tokenMigrationRequired && <span className="hig-badge hig-badge-gold">需迁移 Token</span>}
          {!account.initialized && <span className="hig-badge hig-badge-red">未初始化</span>}
          {account.initialized && (
            <span className={`hig-badge config-chip ${isSelectionMode
              ? "hig-badge-blue"
              : account.has_customized_settings ? "hig-badge-green" : "hig-badge-neutral"}`}>
              {configModeLabel}
            </span>
          )}
        </div>

        <div className="bottom-row">
          {!isSelectionMode && (
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
              ) : account.initialized ? (
                <button
                  onClick={e => {
                    stop(e);
                    if (account.is_running) {
                      void invokeCommand("focus_game_window", { accountId: account.id })
                        .catch(error => showToast("error", `聚焦窗口失败: ${error}`));
                    } else onLaunch(account.id);
                  }}
                  disabled={runtimeStatus?.disabled || !!runtimeStatus?.mode || runtimeStatus?.uncertain || (!account.is_running && !!runtimeStatus?.issue)}
                  className="primary-cta"
                >
                  {account.is_running ? <Locate size={12} /> : <Play size={12} />}
                  {account.is_running ? (english ? "Focus" : "聚焦") : (english ? "Launch" : "启动")}
                </button>
              ) : (
                <button onClick={handleReinit} disabled={reinit} className="primary-cta">
                  {english ? "Continue setup" : "继续配置"}
                </button>
              )}
              {account.initialized && (
                <button
                  type="button"
                  ref={quickAnchor}
                  className="account-quick-toggle"
                  aria-label={english ? `${display}: ${drawerExpanded ? "Close" : "Open"} quick settings` : `${display}：${drawerExpanded ? "收起" : "展开"}快捷配置`}
                  aria-expanded={drawerExpanded}
                  aria-haspopup="dialog"
                  onClick={e => { stop(e); if (expanded) void closeQuick(); else { setMoreOpen(false); handleCardClick(); } }}
                >
                  {english ? "Quick settings" : "快捷配置"} <ChevronDown size={13} aria-hidden="true" />
                </button>
              )}
            <div className="spatial-tools account-card-tools tools">
              {account.initialized && (
                  <button onClick={e => { stop(e); onConfigure(account); }} className="mini-action icon-btn relative" title="高级设置">
                    <Sliders size={12} strokeWidth={1.8} />
                  </button>
              )}
              <button type="button" ref={moreAnchor} className="account-more-trigger"
                aria-label={`${display}：${english ? "More actions" : "更多操作"}`} aria-haspopup="dialog" aria-expanded={moreOpen}
                disabled={runtimeStatus?.disabled || !!runtimeStatus?.mode || reinit}
                onClick={async e => { stop(e); if (expanded && !(await closeQuick())) return; setMoreOpen(!moreOpen); }}>
                <MoreHorizontal size={14} />{english ? "More" : "更多"}
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

      {isSelectionMode ? quickFields : (
        <AnchoredPanel open={expanded} anchor={quickAnchor} title={display + (english ? ' · Quick settings' : ' · 快捷配置')}
          onClose={() => void closeQuick()} closeLabel={english ? 'Close quick settings' : '关闭快捷配置'} className="account-quick-popover" width={460}>
          {quickFields}
          <p className="quick-save-note" role="status">{quickClosing ? (english ? 'Saving changes…' : '正在保存更改…') : drawerLoadError ? (english ? 'Changes are retained. Resolve the error before closing.' : '编辑内容已保留，请处理错误后再关闭。') : (english ? 'Changes save automatically · Apply on next launch' : '更改自动保存 · 下次启动生效')}</p>
        </AnchoredPanel>
      )}
      <AnchoredPanel open={moreOpen} anchor={moreAnchor} title={display} onClose={() => setMoreOpen(false)}
        closeLabel={english ? 'Close actions' : '关闭更多操作'} className="account-actions-popover" width={230}>
        <div className="account-action-list">
          <button type="button" onClick={() => { cancelledRename.current = false; setMoreOpen(false); setEditingName(true); }}>{english ? 'Rename' : '重命名'}</button>
          {account.initialized && account.auth_mode !== 'token' && !tokenMigrationRequired && (
            <button type="button" onClick={() => { setMoreOpen(false); onBattleNetOnly(account.id); }}><Globe2 size={14} />{english ? 'Open Battle.net only' : '仅启动战网'}</button>
          )}
          {account.initialized && <button type="button" onClick={e => { setMoreOpen(false); void handleOpenFolder(e); }}><FolderOpen size={14} />{english ? 'Open account folder' : '打开配置目录'}</button>}
          <button type="button" disabled={reinit} onClick={e => { setMoreOpen(false); void handleReinit(e); }}><RotateCw size={14} />{english ? (account.auth_mode === 'token' ? 'Update Token' : 'Initialize account') : (account.auth_mode === 'token' && account.initialized ? '更新 Token' : reinitializeTitle === '重置' ? '重新初始化' : reinitializeTitle)}</button>
          <button type="button" className="account-action-danger" onClick={handleDelete}><Trash2 size={14} />{english ? 'Delete account…' : '删除账号…'}</button>
        </div>
      </AnchoredPanel>
      <Modal open={confirmDel} onClose={() => setConfirmDel(false)} initialFocusRef={deleteCancel} returnFocusRef={moreAnchor}
        title={english ? 'Delete account?' : '删除账号？'} footer={<>
        <Button ref={deleteCancel} variant="ghost" onClick={() => setConfirmDel(false)}>{english ? 'Cancel' : '取消'}</Button>
        <Button variant="danger" onClick={() => { setConfirmDel(false); onDelete(account.id); }}>{english ? 'Delete account' : '删除账号'}</Button>
      </>}>
        <p className="text-sm text-text-secondary">{english ? 'Delete the local profile for “' + display + '”? This cannot be undone here.' : '确定删除“' + display + '”的本地账号配置吗？删除后无法在这里撤销。'}</p>
      </Modal>
    </div>
  );
}
