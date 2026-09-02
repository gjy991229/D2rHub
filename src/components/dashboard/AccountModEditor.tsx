import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { createPortal } from "react-dom";
import { Check, ChevronDown, PackagePlus } from "lucide-react";

import "../../styles/accountModPicker.css";

import type { AccountMeta, LaunchGroupMember, ModCapsulePool } from "../../store/types";
import {
  capsuleFeatureLabels,
  capsuleSelectionForAccount,
  compatibleCapsulesForAccount,
} from "../../features/modCapsules/model";
import { useI18n } from "../../i18n";

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

function legacyLabel(argumentsValue: string, isEnglish: boolean): string {
  const match = /(?:^|\s)-mod(?:=|\s+)(?:"([^"]+)"|'([^']+)'|([^\s]+))/i.exec(argumentsValue);
  return match?.[1] || match?.[2] || match?.[3] || (isEnglish ? "Custom arguments" : "自定义参数");
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
  const { language } = useI18n();
  const isEnglish = language === "en-US";
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState<{ left: number; top: number; opensUpward: boolean } | null>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const menuRef = useRef<HTMLDivElement | null>(null);
  const menuId = `account-mod-picker-${useId().replaceAll(":", "")}`;
  const activeArguments = isSelectionMode ? schemeMember?.mod_args ?? "" : account.mod_args;
  const selection = capsuleSelectionForAccount(modCapsulePool, account.id);
  const capsules = compatibleCapsulesForAccount(modCapsulePool, account.id);
  const selected = capsules.find((capsule) => capsule.launch_arguments.trim() === activeArguments.trim()) ?? null;
  const activeLabel = activeArguments.trim()
    ? selected?.name ?? legacyLabel(activeArguments, isEnglish)
    : isEnglish ? "Original" : "原版";

  const updatePosition = useCallback(() => {
    const trigger = triggerRef.current;
    if (!trigger) return;
    const rect = trigger.getBoundingClientRect();
    const width = 330;
    const height = Math.min(280, 118 + Math.ceil(Math.max(1, capsules.length) / 3) * 34);
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
    const frame = window.requestAnimationFrame(() => {
      const items = Array.from(menuRef.current?.querySelectorAll<HTMLButtonElement>(
        'button:not([disabled])',
      ) ?? []);
      (items.find((item) => item.getAttribute("aria-checked") === "true") ?? items[0])?.focus();
    });
    return () => window.cancelAnimationFrame(frame);
  }, [open, position]);

  const handleMenuKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    const items = Array.from(menuRef.current?.querySelectorAll<HTMLButtonElement>(
      'button:not([disabled])',
    ) ?? []);
    const currentIndex = items.indexOf(document.activeElement as HTMLButtonElement);
    const nextIndex = event.key === "Home"
      ? 0
      : event.key === "End"
        ? items.length - 1
        : event.key === "ArrowDown"
          ? (currentIndex + 1 + items.length) % items.length
          : event.key === "ArrowUp"
            ? (currentIndex - 1 + items.length) % items.length
            : null;
    if (nextIndex === null || items.length === 0) return;
    event.preventDefault();
    items[nextIndex]?.focus();
  };

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
        title={isEnglish ? `Current Mod: ${activeLabel}. Select a Mod` : `当前 Mod：${activeLabel}；点击选择 Mod`}
        onClick={() => setOpen((current) => !current)}
        onKeyDown={(event) => {
          if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
          event.preventDefault();
          setOpen(true);
        }}
      >
        <span>{activeLabel}</span>
        <ChevronDown size={10} aria-hidden="true" />
      </button>
      <button
        type="button"
        className="hig-badge mod-chip account-mod-manage-trigger"
        title={isEnglish ? "Open Mod Management to add shared arguments" : "前往 Mod 管理新增共享参数"}
        onClick={() => onOpenModManager?.("add", selection?.edition)}
      >
        <PackagePlus size={11} aria-hidden="true" />
      </button>
      {open && position && createPortal(
        <div
          ref={menuRef}
          id={menuId}
          role="menu"
          aria-label={isEnglish
            ? `Select a Mod for ${account.display_name || account.id}`
            : `${account.display_name || account.id} 选择 Mod`}
          className="account-mod-picker"
          data-placement={position.opensUpward ? "top" : "bottom"}
          style={{ left: position.left, top: position.top }}
          onKeyDown={handleMenuKeyDown}
        >
          <div className="account-mod-picker-heading">
            <div><strong>{isEnglish ? "Select Mod" : "选择 Mod"}</strong><span>{selection?.edition ?? (isEnglish ? "Edition unknown" : "版本未确定")}</span></div>
            <button type="button" role="menuitem" onClick={() => { close(); onOpenModManager?.(); }}>{isEnglish ? "Manage" : "管理"}</button>
          </div>
          <div className="account-mod-picker-capsules">
            <button
              type="button"
              role="menuitemradio"
              aria-checked={!activeArguments.trim()}
              className="account-mod-picker-capsule"
              data-active={!activeArguments.trim() ? "true" : undefined}
              onClick={() => void choose(null, "")}
            >
              {isEnglish ? "Original game" : "原版游戏"}
              {!activeArguments.trim() && <Check size={11} aria-hidden="true" />}
            </button>
            {capsules.map((capsule) => {
              const active = capsule.launch_arguments.trim() === activeArguments.trim();
              const features = capsuleFeatureLabels(capsule, isEnglish);
              return (
                <button
                  type="button"
                  role="menuitemradio"
                  aria-checked={active}
                  className="account-mod-picker-capsule"
                  data-active={active ? "true" : undefined}
                  data-processed={capsule.processed ? "true" : undefined}
                  key={capsule.id}
                  title={`${capsule.name}${features.length ? ` · ${features.join("、")}` : ""}`}
                  onClick={() => void choose(capsule.id, capsule.launch_arguments)}
                >
                  <span>{capsule.name}</span>
                  {active && <Check size={11} aria-hidden="true" />}
                </button>
              );
            })}
          </div>
          {capsules.length === 0 && <p className="account-mod-picker-empty">
            {isEnglish ? "No Mods are available for this edition. Scan or add one in Mod Management." : "当前版本还没有可用 Mod，请前往 Mod 管理扫描或新增。"}
          </p>}
        </div>,
        document.body,
      )}
    </div>
  );
}
