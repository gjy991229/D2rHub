import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Check, ChevronDown, PackagePlus } from "lucide-react";

import type { AccountMeta, LaunchGroupMember, ModCapsulePool } from "../../store/types";
import {
  capsuleSelectionForAccount,
  compatibleCapsulesForAccount,
} from "../../features/modCapsules/model";

interface AccountModEditorProps {
  account: AccountMeta;
  modCapsulePool?: ModCapsulePool | null;
  assigning?: boolean;
  isSelectionMode?: boolean;
  schemeMember?: LaunchGroupMember;
  onSchemeMemberChange?: (id: string, patch: Partial<LaunchGroupMember>) => void;
  onAssign?: (accountId: string, capsuleId: string | null) => Promise<unknown>;
  onOpenModManager?: (action?: "add", edition?: string | null) => void;
}

function legacyLabel(argumentsValue: string): string {
  const match = /(?:^|\s)-mod(?:=|\s+)(?:"([^"]+)"|'([^']+)'|([^\s]+))/i.exec(argumentsValue);
  return match?.[1] || match?.[2] || match?.[3] || "自定义参数";
}

export function AccountModEditor({
  account,
  modCapsulePool = null,
  assigning = false,
  isSelectionMode,
  schemeMember,
  onSchemeMemberChange,
  onAssign,
  onOpenModManager,
}: AccountModEditorProps) {
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState<{ left: number; top: number; opensUpward: boolean } | null>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const menuRef = useRef<HTMLDivElement | null>(null);
  const menuId = `account-mod-picker-${useId().replaceAll(":", "")}`;
  const activeArguments = isSelectionMode ? schemeMember?.mod_args ?? "" : account.mod_args;
  const selection = capsuleSelectionForAccount(modCapsulePool, account.id);
  const capsules = compatibleCapsulesForAccount(modCapsulePool, account.id);
  const selected = capsules.find((capsule) => capsule.launch_arguments.trim() === activeArguments.trim()) ?? null;
  const activeLabel = activeArguments.trim() ? selected?.name ?? legacyLabel(activeArguments) : "原版";

  const updatePosition = useCallback(() => {
    const trigger = triggerRef.current;
    if (!trigger) return;
    const rect = trigger.getBoundingClientRect();
    const width = 330;
    const height = Math.min(360, 104 + Math.max(1, capsules.length) * 52);
    const gap = 6;
    const padding = 8;
    const opensUpward = window.innerHeight - rect.bottom < height + gap + padding
      && rect.top > height + gap + padding;
    setPosition({
      left: Math.min(window.innerWidth - width - padding, Math.max(padding, rect.left)),
      top: opensUpward ? Math.max(padding, rect.top - height - gap) : rect.bottom + gap,
      opensUpward,
    });
  }, [capsules.length]);

  const close = useCallback((restoreFocus = false) => {
    setOpen(false);
    setPosition(null);
    if (restoreFocus) window.requestAnimationFrame(() => triggerRef.current?.focus());
  }, []);

  useLayoutEffect(() => {
    if (open) updatePosition();
  }, [open, updatePosition]);

  useEffect(() => {
    if (!open || !position) return;
    const outside = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) return;
      if (triggerRef.current?.contains(target) || menuRef.current?.contains(target)) return;
      close();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") close(true);
    };
    document.addEventListener("pointerdown", outside, true);
    document.addEventListener("keydown", escape);
    window.addEventListener("resize", updatePosition);
    return () => {
      document.removeEventListener("pointerdown", outside, true);
      document.removeEventListener("keydown", escape);
      window.removeEventListener("resize", updatePosition);
    };
  }, [close, open, position, updatePosition]);

  const choose = async (capsuleId: string | null, launchArguments: string) => {
    if (isSelectionMode) {
      onSchemeMemberChange?.(account.id, { mod_args: launchArguments });
    } else {
      await onAssign?.(account.id, capsuleId);
    }
    close(true);
  };

  return (
    <div className="account-mod-selector" onClick={(event) => event.stopPropagation()}>
      <button
        ref={triggerRef}
        type="button"
        className="hig-badge mod-chip account-mod-selector-trigger"
        data-processed={selected?.processed ? "true" : undefined}
        disabled={assigning}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        title={`当前 Mod：${activeLabel}；点击选择共享 Mod`}
        onClick={() => setOpen((current) => !current)}
      >
        <span>{activeLabel}</span>
        <ChevronDown size={10} aria-hidden="true" />
      </button>
      <button
        type="button"
        className="hig-badge mod-chip account-mod-manage-trigger"
        title="前往 Mod 管理新增共享参数"
        onClick={() => onOpenModManager?.("add", selection?.edition)}
      >
        <PackagePlus size={11} aria-hidden="true" />
      </button>
      {open && position && createPortal(
        <div
          ref={menuRef}
          id={menuId}
          role="menu"
          aria-label={`${account.display_name || account.id} 选择共享 Mod`}
          className="account-mod-picker"
          data-placement={position.opensUpward ? "top" : "bottom"}
          style={{ left: position.left, top: position.top }}
        >
          <div className="account-mod-picker-heading">
            <div><strong>选择共享 Mod</strong><span>{selection?.edition ?? "版本未确定"}</span></div>
            <button type="button" onClick={() => { close(); onOpenModManager?.(); }}>管理</button>
          </div>
          <button
            type="button"
            role="menuitemradio"
            aria-checked={!activeArguments.trim()}
            className="account-mod-picker-option"
            onClick={() => void choose(null, "")}
          >
            <span><strong>原版游戏</strong><small>不使用 Mod，也不保存“无 Mod”胶囊</small></span>
            {!activeArguments.trim() && <Check size={13} aria-hidden="true" />}
          </button>
          {capsules.map((capsule) => {
            const active = capsule.launch_arguments.trim() === activeArguments.trim();
            return (
              <button
                type="button"
                role="menuitemradio"
                aria-checked={active}
                className="account-mod-picker-option"
                data-processed={capsule.processed ? "true" : undefined}
                key={capsule.id}
                onClick={() => void choose(capsule.id, capsule.launch_arguments)}
              >
                <span>
                  <strong>{capsule.name}</strong>
                  <small>{capsule.origin === "scanned" ? "游戏目录预设" : "自定义共享参数"} · {capsule.launch_arguments}</small>
                </span>
                {active ? <Check size={13} aria-hidden="true" /> : capsule.processed ? <em>已加工</em> : null}
              </button>
            );
          })}
          {capsules.length === 0 && <p className="account-mod-picker-empty">当前版本还没有可用 Mod，请前往 Mod 管理扫描或新增。</p>}
        </div>,
        document.body,
      )}
    </div>
  );
}
