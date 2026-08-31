import React, { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  AlertTriangle,
  FolderOpen,
  Globe2,
  Locate,
  Play,
  RotateCw,
  Sliders,
  Trash2,
  X,
} from "lucide-react";

import type {
  AccountMeta,
  GlobalConfig,
  LaunchGroupMember,
  LaunchProgress,
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
  getModSchemeUsage?: (id: string, modArgs: string) => string[];
  getPositionSchemeUsage?: (id: string, positionId: string) => string[];
  onUpdateToken?: (a: AccountMeta) => void;
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

function getModChipLabel(mod: string): string {
  const tokens = mod.match(/"[^"]*"|'[^']*'|\S+/g) || [];
  const names = tokens
    .map(token => token.replace(/^(['"])(.*)\1$/, "$2"))
    .filter(token => token && !token.startsWith("-"));

  return names.join(" ") || mod;
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
  isSelectionMode, selected, onToggleSelect, schemeMember, onSchemeMemberChange,
  getModSchemeUsage, getPositionSchemeUsage, onUpdateToken, config,
}: GridItemProps) {
  const display = account.display_name || account.id;
  const [editingName, setEditingName] = useState(false);
  const [nameDraft, setNameDraft] = useState(display);
  const [confirmDel, setConfirmDel] = useState(false);
  const [modDelConfirmIdx, setModDelConfirmIdx] = useState<number | null>(null);
  const [reinit, setReinit] = useState(false);
  const [modEditing, setModEditing] = useState(false);
  const [modDraft, setModDraft] = useState(account.mod_args || "");
  const [positionEditing, setPositionEditing] = useState(false);
  const [positionNameDraft, setPositionNameDraft] = useState("");
  const [positionXDraft, setPositionXDraft] = useState("0");
  const [positionYDraft, setPositionYDraft] = useState("0");
  const [positionDelConfirmId, setPositionDelConfirmId] = useState<string | null>(null);
  const {
    addAccountMod,
    reinitializeAccount,
    updateAccountMods,
    updateAccountPositions,
  } = useAccounts();

  // ── 抽屉配置面板 ──
  const [expanded, setExpanded] = useState(false);
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
  const modRowDragRef = useRef<{
    pointerId: number;
    startX: number;
    scrollLeft: number;
    moved: boolean;
    captured: boolean;
  } | null>(null);
  const suppressModClickRef = useRef(false);
  const suppressModClickTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const modCommitInFlightRef = useRef(false);
  const nameCommitInFlightRef = useRef(false);

  useEffect(() => { setModDraft(account.mod_args || ""); }, [account.mod_args]);

  useEffect(() => () => {
    if (suppressModClickTimerRef.current) {
      clearTimeout(suppressModClickTimerRef.current);
    }
  }, []);

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

  const commitMod = async () => {
    if (modCommitInFlightRef.current) return;
    setModEditing(false);
    const v = modDraft.trim();
    if (!v) return;

    modCommitInFlightRef.current = true;
    try {
      if (isSelectionMode) {
        const existing = (account.mod_list || []).find(mod => mod.trim() === v);
        if (existing) {
          onSchemeMemberChange?.(account.id, { mod_args: existing });
          showToast("info", "已有完全相同的 Mod 配置，已为当前方案选中");
          return;
        }
        const saved = await updateAccountMods(
          account.id,
          account.mod_args,
          [...(account.mod_list || []), v],
        );
        if (saved) onSchemeMemberChange?.(account.id, { mod_args: v });
        return;
      }
      const added = await addAccountMod(account.id, v);
      if (added === false) {
        showToast("info", "已有完全相同的 Mod 配置，已跳过添加");
      }
    } finally {
      modCommitInFlightRef.current = false;
    }
  };

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
    setPositionEditing(false);
    setPositionNameDraft("");
  };

  const deletePosition = async (position: WindowPositionPreset) => {
    const usedBy = getPositionSchemeUsage?.(account.id, position.id) ?? [];
    if (usedBy.length > 0) {
      showToast("warning", `位置“${position.name}”正被方案“${usedBy.join("、")}”使用，请先更换方案配置`);
      setPositionDelConfirmId(null);
      return;
    }
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

  const commitName = async () => {
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
      await invoke("open_account_dir", { accountId: account.id });
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
  const performanceLabel = `${effectiveResolution} · ${effectiveFps === 0 ? "unlimited" : `${effectiveFps}fps`}`;
  const configModeLabel = isSelectionMode
    ? "方案画质"
    : account.has_customized_settings ? "独立配置" : "系统配置";
  const activeMod = isSelectionMode ? schemeMember?.mod_args ?? "" : account.mod_args;
  const drawerExpanded = isSelectionMode ? !!selected : expanded;

  // ── 预置选项 ──
  const resOptions = ["1280x720","1600x900","1920x1080","2560x1440","3840x2160"];
  const effectiveResOptions = resOptions.includes(effectiveResolution)
    ? resOptions
    : [effectiveResolution, ...resOptions];
  const fpsOptions = [0, 30, 60, 120, 144, 240];
  const stop = (e: React.MouseEvent) => e.stopPropagation();

  const handleModWheel = (event: React.WheelEvent<HTMLDivElement>) => {
    const row = event.currentTarget;
    if (row.scrollWidth <= row.clientWidth) return;

    const delta = Math.abs(event.deltaX) > Math.abs(event.deltaY) ? event.deltaX : event.deltaY;
    if (delta === 0) return;

    const maxScrollLeft = row.scrollWidth - row.clientWidth;
    const nextScrollLeft = Math.min(maxScrollLeft, Math.max(0, row.scrollLeft + delta));
    if (nextScrollLeft === row.scrollLeft) return;

    event.preventDefault();
    event.stopPropagation();
    row.scrollLeft = nextScrollLeft;
  };

  const handleModPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    event.stopPropagation();
    if (event.button !== 0 || (event.target as HTMLElement).closest("input")) return;

    const row = event.currentTarget;
    if (row.scrollWidth <= row.clientWidth) return;

    suppressModClickRef.current = false;
    modRowDragRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      scrollLeft: row.scrollLeft,
      moved: false,
      captured: false,
    };
  };

  const handleModPointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const drag = modRowDragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;

    const distance = event.clientX - drag.startX;
    if (!drag.moved && Math.abs(distance) >= 4) {
      drag.moved = true;
      drag.captured = true;
      event.currentTarget.setPointerCapture(event.pointerId);
      event.currentTarget.dataset.dragging = "true";
    }
    if (!drag.moved) return;

    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.scrollLeft = drag.scrollLeft - distance;
  };

  const finishModPointerDrag = (event: React.PointerEvent<HTMLDivElement>) => {
    const drag = modRowDragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;

    const row = event.currentTarget;
    modRowDragRef.current = null;
    delete row.dataset.dragging;
    if (drag.captured && row.hasPointerCapture(event.pointerId)) {
      row.releasePointerCapture(event.pointerId);
    }
    event.stopPropagation();

    if (drag.moved) {
      suppressModClickRef.current = true;
      if (suppressModClickTimerRef.current) clearTimeout(suppressModClickTimerRef.current);
      suppressModClickTimerRef.current = setTimeout(() => {
        suppressModClickRef.current = false;
        suppressModClickTimerRef.current = null;
      }, 0);
    }
  };

  const handleModClickCapture = (event: React.MouseEvent<HTMLDivElement>) => {
    if (!suppressModClickRef.current) return;
    suppressModClickRef.current = false;
    event.preventDefault();
    event.stopPropagation();
  };

  const handleModKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget) return;
    const row = event.currentTarget;
    if (row.scrollWidth <= row.clientWidth) return;

    let nextScrollLeft: number | null = null;
    if (event.key === "ArrowLeft") nextScrollLeft = row.scrollLeft - 72;
    if (event.key === "ArrowRight") nextScrollLeft = row.scrollLeft + 72;
    if (event.key === "Home") nextScrollLeft = 0;
    if (event.key === "End") nextScrollLeft = row.scrollWidth;
    if (nextScrollLeft === null) return;

    event.preventDefault();
    event.stopPropagation();
    row.scrollTo({ left: nextScrollLeft, behavior: "smooth" });
  };

  return (
    <div
      onClick={handleCardClick}
      className="spatial-tile group account-tile flex min-h-[152px] flex-col animate-card-in"
      data-expanded={expanded ? "true" : "false"}
      data-selected={selected ? "true" : "false"}
      data-scheme-edit={isSelectionMode ? "true" : undefined}
      style={{
        cursor: isSelectionMode
          ? (selected || (account.initialized && !tokenMigrationRequired) ? "pointer" : "not-allowed")
          : (account.initialized ? "pointer" : "default"),
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
                  {selected && <span className="scheme-context-label">方案配置</span>}
                </>
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
                  onKeyDown={e => { if (e.key === "Enter") void commitName(); if (e.key === "Escape") { setNameDraft(display); setEditingName(false); } }}
                  onBlur={() => { void commitName(); }}
                  onClick={stop}
                  autoFocus
                />
              ) : (
                <button
                  className="tile-name name min-w-0 max-w-full text-left transition-colors duration-200 hover:text-text-secondary"
                  onClick={e => {
                    if (isSelectionMode) return;
                    e.stopPropagation();
                    setEditingName(true);
                  }}
                >
                  <span data-i18n-skip>{display}</span>
                </button>
              )}

              {(!isSelectionMode || selected) && (
                <div
                  className="mod-row inline-mod-row"
                  role="group"
                  aria-label="Mod 配置，横向滚动查看更多"
                  tabIndex={0}
                  onWheel={handleModWheel}
                  onPointerDown={handleModPointerDown}
                  onPointerMove={handleModPointerMove}
                  onPointerUp={finishModPointerDrag}
                  onPointerCancel={finishModPointerDrag}
                  onLostPointerCapture={finishModPointerDrag}
                  onClickCapture={handleModClickCapture}
                  onKeyDown={handleModKeyDown}
                >
                  <button
                    type="button"
                    onClick={event => {
                      event.stopPropagation();
                      if (activeMod === "") return;
                      if (isSelectionMode) {
                        onSchemeMemberChange?.(account.id, { mod_args: "" });
                      } else {
                        void updateAccountMods(account.id, "", account.mod_list || []);
                      }
                    }}
                    className={`hig-badge mod-chip shrink-0 active:scale-[0.97] ${activeMod === "" ? "mod-chip-active" : ""}`}
                    title={activeMod === "" ? "当前不使用 Mod" : "不使用 Mod"}
                  >
                    无 Mod
                  </button>
                  {(account.mod_list || []).map((mod, idx) => {
                    const isActive = activeMod === mod;
                    const isConfirming = modDelConfirmIdx === idx;
                    const modLabel = getModChipLabel(mod);
                    return (
                      <div key={idx} className="group/mod relative flex items-center" onMouseLeave={() => setModDelConfirmIdx(null)}>
                        <button
                          onClick={e => {
                            e.stopPropagation();
                            if (!isActive) {
                              if (isSelectionMode) {
                                onSchemeMemberChange?.(account.id, { mod_args: mod });
                              } else {
                                void updateAccountMods(account.id, mod, account.mod_list || []);
                              }
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
                              const usedBy = getModSchemeUsage?.(account.id, mod) ?? [];
                              if (usedBy.length > 0) {
                                showToast("warning", `Mod“${modLabel}”正被方案“${usedBy.join("、")}”使用，请先更换方案配置`);
                                setModDelConfirmIdx(null);
                                return;
                              }
                              const newMods = (account.mod_list || []).filter((_, i) => i !== idx);
                              const nextActive = account.mod_args === mod ? (newMods[0] || "") : account.mod_args;
                              void updateAccountMods(account.id, nextActive, newMods).then(saved => {
                                if (saved && isSelectionMode && isActive) {
                                  onSchemeMemberChange?.(account.id, { mod_args: "" });
                                }
                              });
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
                  {(modEditing ? (
                    <input
                      className="line-input h-[24px] w-28 px-2 font-mono text-xs"
                      value={modDraft}
                      onChange={e => setModDraft(e.target.value)}
                      onKeyDown={e => {
                        if (e.key === "Enter") {
                          e.stopPropagation();
                          void commitMod();
                        }
                        if (e.key === "Escape") {
                          e.stopPropagation();
                          setModDraft("");
                          setModEditing(false);
                        }
                      }}
                      onBlur={() => { void commitMod(); }}
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
                  ))}
                </div>
              )}
            </div>
          </div>
          <span className={account.initialized && !tokenMigrationRequired ? "state-dot state" : "state-dot state warn"} />

        </div>

        <div className="tag-row tag-row-offset">
          <span className="hig-badge hig-badge-neutral">{lastLaunchText || (account.initialized ? "已就绪" : "待配置")}</span>
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
          {account.auth_mode === "token" && <span className="hig-badge hig-badge-green">长期</span>}
          {!account.initialized && <span className="hig-badge hig-badge-red">未初始化</span>}
        </div>

        <div className="tag-row tag-row-secondary tag-row-offset">
          {drawerLoaded && account.initialized && <span className="hig-badge hig-badge-neutral performance-chip">{performanceLabel}</span>}
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
              ) : account.initialized && (
                <button
                  onClick={e => { stop(e); onLaunch(account.id); }}
                  className="primary-cta"
                >
                  <Play size={12} />
                  启动
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
          gridTemplateRows: drawerExpanded ? "1fr" : "0fr",
          transition: "grid-template-rows 0.32s cubic-bezier(0.16, 1, 0.3, 1)",
        }}
      >
        <div style={{ overflow: "hidden", minHeight: 0 }}>
          <div className="drawer-body">
            <div className="drawer-grid">
              {drawerLoadError && (
                <div className="scheme-settings-error hig-badge hig-badge-red" title={drawerLoadError}>
                  画质配置{drawerLoaded ? "保存" : "读取"}失败，请检查 Settings.json
                </div>
              )}
              <div className="drawer-resolution-fps-row">
                <div className="drawer-field">
                  <label className="micro-meta mb-1.5 block">分辨率</label>
                  <select
                    value={effectiveResolution}
                    onChange={e => selectDrawerSetting("resolution", e.target.value)}
                    onBlur={() => void flushDrawerSettings().catch(() => undefined)}
                    onClick={stop}
                    disabled={drawerLoading}
                    className="line-select w-full px-2.5"
                  >
                    {effectiveResOptions.map(r => <option key={r} value={r}>{r}</option>)}
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
                  <button
                    type="button"
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
                    title="添加位置"
                  >
                    +
                  </button>
                  {!isSelectionMode && <button
                    onClick={async (e) => {
                      e.stopPropagation();
                      try {
                        await invoke("move_game_window", { accountId: account.id });
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
                  <div className="position-preset-editor" onClick={stop}>
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
                          if (event.key === "Escape") setPositionEditing(false);
                        }}
                      />
                    </label>
                    <button type="button" className="primary-cta" onClick={() => void commitPosition()}>
                      保存位置
                    </button>
                    <button type="button" className="control-btn" onClick={() => setPositionEditing(false)}>
                      取消
                    </button>
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
