import {
  type KeyboardEvent,
  type MouseEvent,
  type PointerEvent,
  type WheelEvent,
  useEffect,
  useRef,
  useState,
} from "react";
import { X } from "lucide-react";

import { useAccounts } from "../../store/accounts";
import type { AccountMeta, LaunchGroupMember } from "../../store/types";
import { showToast } from "../ui/Toast";

interface AccountModEditorProps {
  account: AccountMeta;
  isSelectionMode?: boolean;
  schemeMember?: LaunchGroupMember;
  onSchemeMemberChange?: (id: string, patch: Partial<LaunchGroupMember>) => void;
  getModSchemeUsage?: (id: string, modArgs: string) => string[];
}

function getModChipLabel(mod: string): string {
  const tokens = mod.match(/"[^"]*"|'[^']*'|\S+/g) || [];
  const names = tokens
    .map(token => token.replace(/^(['"])(.*)\1$/, "$2"))
    .filter(token => token && !token.startsWith("-"));

  return names.join(" ") || mod;
}

export function AccountModEditor({
  account,
  isSelectionMode,
  schemeMember,
  onSchemeMemberChange,
  getModSchemeUsage,
}: AccountModEditorProps) {
  const { addAccountMod, updateAccountMods } = useAccounts();
  const [confirmDeleteIndex, setConfirmDeleteIndex] = useState<number | null>(null);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(account.mod_args || "");
  const dragRef = useRef<{
    pointerId: number;
    startX: number;
    scrollLeft: number;
    moved: boolean;
    captured: boolean;
  } | null>(null);
  const suppressClickRef = useRef(false);
  const suppressClickTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const commitInFlightRef = useRef(false);
  const activeMod = isSelectionMode ? schemeMember?.mod_args ?? "" : account.mod_args;

  useEffect(() => setDraft(account.mod_args || ""), [account.mod_args]);

  useEffect(() => () => {
    if (suppressClickTimerRef.current) clearTimeout(suppressClickTimerRef.current);
  }, []);

  const commit = async () => {
    if (commitInFlightRef.current) return;
    setEditing(false);
    const value = draft.trim();
    if (!value) return;

    commitInFlightRef.current = true;
    try {
      if (isSelectionMode) {
        const existing = (account.mod_list || []).find(mod => mod.trim() === value);
        if (existing) {
          onSchemeMemberChange?.(account.id, { mod_args: existing });
          showToast("info", "已有完全相同的 Mod 配置，已为当前方案选中");
          return;
        }
        const saved = await updateAccountMods(
          account.id,
          account.mod_args,
          [...(account.mod_list || []), value],
        );
        if (saved) onSchemeMemberChange?.(account.id, { mod_args: value });
        return;
      }
      const added = await addAccountMod(account.id, value);
      if (added === false) showToast("info", "已有完全相同的 Mod 配置，已跳过添加");
    } finally {
      commitInFlightRef.current = false;
    }
  };

  const handleWheel = (event: WheelEvent<HTMLDivElement>) => {
    const row = event.currentTarget;
    if (row.scrollWidth <= row.clientWidth) return;
    const delta = Math.abs(event.deltaX) > Math.abs(event.deltaY) ? event.deltaX : event.deltaY;
    if (delta === 0) return;
    const next = Math.min(row.scrollWidth - row.clientWidth, Math.max(0, row.scrollLeft + delta));
    if (next === row.scrollLeft) return;
    event.preventDefault();
    event.stopPropagation();
    row.scrollLeft = next;
  };

  const handlePointerDown = (event: PointerEvent<HTMLDivElement>) => {
    event.stopPropagation();
    if (event.button !== 0 || (event.target as HTMLElement).closest("input")) return;
    const row = event.currentTarget;
    if (row.scrollWidth <= row.clientWidth) return;
    suppressClickRef.current = false;
    dragRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      scrollLeft: row.scrollLeft,
      moved: false,
      captured: false,
    };
  };

  const handlePointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
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

  const finishPointerDrag = (event: PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    const row = event.currentTarget;
    dragRef.current = null;
    delete row.dataset.dragging;
    if (drag.captured && row.hasPointerCapture(event.pointerId)) {
      row.releasePointerCapture(event.pointerId);
    }
    event.stopPropagation();
    if (!drag.moved) return;

    suppressClickRef.current = true;
    if (suppressClickTimerRef.current) clearTimeout(suppressClickTimerRef.current);
    suppressClickTimerRef.current = setTimeout(() => {
      suppressClickRef.current = false;
      suppressClickTimerRef.current = null;
    }, 0);
  };

  const handleClickCapture = (event: MouseEvent<HTMLDivElement>) => {
    if (!suppressClickRef.current) return;
    suppressClickRef.current = false;
    event.preventDefault();
    event.stopPropagation();
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget) return;
    const row = event.currentTarget;
    if (row.scrollWidth <= row.clientWidth) return;
    let next: number | null = null;
    if (event.key === "ArrowLeft") next = row.scrollLeft - 72;
    if (event.key === "ArrowRight") next = row.scrollLeft + 72;
    if (event.key === "Home") next = 0;
    if (event.key === "End") next = row.scrollWidth;
    if (next === null) return;
    event.preventDefault();
    event.stopPropagation();
    row.scrollTo({ left: next, behavior: "smooth" });
  };

  return (
    <div
      className="mod-row inline-mod-row"
      role="group"
      aria-label="Mod 配置，横向滚动查看更多"
      tabIndex={0}
      onWheel={handleWheel}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={finishPointerDrag}
      onPointerCancel={finishPointerDrag}
      onLostPointerCapture={finishPointerDrag}
      onClickCapture={handleClickCapture}
      onKeyDown={handleKeyDown}
    >
      <button
        type="button"
        onClick={event => {
          event.stopPropagation();
          if (activeMod === "") return;
          if (isSelectionMode) onSchemeMemberChange?.(account.id, { mod_args: "" });
          else void updateAccountMods(account.id, "", account.mod_list || []);
        }}
        className={`hig-badge mod-chip shrink-0 active:scale-[0.97] ${activeMod === "" ? "mod-chip-active" : ""}`}
        title={activeMod === "" ? "当前不使用 Mod" : "不使用 Mod"}
      >
        无 Mod
      </button>
      {(account.mod_list || []).map((mod, index) => {
        const active = activeMod === mod;
        const confirming = confirmDeleteIndex === index;
        const label = getModChipLabel(mod);
        return (
          <div key={mod} className="group/mod relative flex items-center" onMouseLeave={() => setConfirmDeleteIndex(null)}>
            <button
              onClick={event => {
                event.stopPropagation();
                if (active) return;
                if (isSelectionMode) onSchemeMemberChange?.(account.id, { mod_args: mod });
                else void updateAccountMods(account.id, mod, account.mod_list || []);
              }}
              className={`hig-badge mod-chip max-w-[118px] truncate font-mono active:scale-[0.97] ${active ? "mod-chip-active" : ""}`}
              title={active ? `${mod}（当前生效）` : `${mod}（点击生效）`}
            >
              {label}
            </button>
            <button
              className={`absolute -right-1.5 -top-1.5 z-10 flex h-4 w-4 items-center justify-center rounded-full transition-all ${
                confirming
                  ? "scale-110 text-white opacity-100"
                  : "text-text-muted opacity-0 hover:scale-110 group-hover/mod:opacity-100"
              }`}
              style={{
                background: confirming ? "var(--error)" : "var(--surface-glass)",
                border: "1px solid var(--border-default)",
              }}
              onClick={event => {
                event.stopPropagation();
                if (!confirming) {
                  setConfirmDeleteIndex(index);
                  return;
                }
                const usedBy = getModSchemeUsage?.(account.id, mod) ?? [];
                if (usedBy.length > 0) {
                  showToast("warning", `Mod“${label}”正被方案“${usedBy.join("、")}”使用，请先更换方案配置`);
                  setConfirmDeleteIndex(null);
                  return;
                }
                const nextMods = (account.mod_list || []).filter((_, candidate) => candidate !== index);
                const nextActive = account.mod_args === mod ? (nextMods[0] || "") : account.mod_args;
                void updateAccountMods(account.id, nextActive, nextMods).then(saved => {
                  if (saved && isSelectionMode && active) {
                    onSchemeMemberChange?.(account.id, { mod_args: "" });
                  }
                });
                setConfirmDeleteIndex(null);
              }}
              title={confirming ? "确认删除" : "删除配置"}
            >
              <X size={9} />
            </button>
          </div>
        );
      })}
      {editing ? (
        <input
          className="line-input h-[24px] w-28 px-2 font-mono text-xs"
          value={draft}
          onChange={event => setDraft(event.target.value)}
          onKeyDown={event => {
            if (event.key === "Enter") {
              event.stopPropagation();
              void commit();
            }
            if (event.key === "Escape") {
              event.stopPropagation();
              setDraft("");
              setEditing(false);
            }
          }}
          onBlur={() => void commit()}
          onClick={event => event.stopPropagation()}
          placeholder="-mod xxx"
          autoFocus
        />
      ) : (
        <button
          onClick={event => {
            event.stopPropagation();
            setDraft("");
            setEditing(true);
          }}
          className="hig-badge mod-chip w-[24px] justify-center px-0"
          title="添加 mod"
        >
          +
        </button>
      )}
    </div>
  );
}
