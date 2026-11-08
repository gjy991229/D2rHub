import { type ReactNode, type RefObject, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";

// Only the top dialog receives Escape/Tab when settings opens a nested flow.
const openDialogs: symbol[] = [];

interface ModalProps {
  open: boolean;
  onClose: () => void;
  title?: string;
  children: ReactNode;
  footer?: ReactNode;
  width?: string;
  closeOnContextMenu?: boolean;
  dismissible?: boolean;
  initialFocusRef?: RefObject<HTMLElement | null>;
  returnFocusRef?: RefObject<HTMLElement | null>;
}

export function Modal({
  open,
  onClose,
  title,
  children,
  footer,
  width = "max-w-md",
  closeOnContextMenu = false,
  dismissible = true,
  initialFocusRef,
  returnFocusRef,
}: ModalProps) {
  const backdropRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const dialogId = useRef(Symbol("dialog"));
  const [mounted, setMounted] = useState(open);
  const [closing, setClosing] = useState(false);

  useEffect(() => {
    if (!open) return;
    const id = dialogId.current;
    openDialogs.push(id);
    return () => {
      const index = openDialogs.indexOf(id);
      if (index !== -1) openDialogs.splice(index, 1);
    };
  }, [open]);

  useEffect(() => {
    if (open) {
      setMounted(true);
      setClosing(false);
      return;
    }

    if (!mounted) return;
    setClosing(true);
    const timer = window.setTimeout(() => {
      setMounted(false);
      setClosing(false);
    }, 180);
    return () => window.clearTimeout(timer);
  }, [open, mounted]);

  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (openDialogs[openDialogs.length - 1] !== dialogId.current) return;
      if (e.key === "Escape" && dismissible) {
        e.preventDefault();
        onClose();
        return;
      }
      if (e.key !== "Tab") return;

      const content = contentRef.current;
      if (!content) return;
      const focusable = Array.from(content.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), summary, [tabindex]:not([tabindex="-1"])',
      )).filter(element => {
        if (element.closest("[hidden], [inert]")) return false;
        for (let ancestor: HTMLElement | null = element; ancestor && ancestor !== content; ancestor = ancestor.parentElement) {
          const style = getComputedStyle(ancestor);
          if (style.display === "none" || style.visibility === "hidden") return false;
          if (ancestor instanceof HTMLDetailsElement && !ancestor.open
            && !ancestor.querySelector(":scope > summary")?.contains(element)) return false;
        }
        return true;
      });
      if (focusable.length === 0) {
        e.preventDefault();
        content.focus();
        return;
      }

      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (e.shiftKey && (document.activeElement === first || document.activeElement === content)) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [dismissible, open, onClose]);

  useEffect(() => {
    if (!open) return;
    const previousFocus = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    const frame = window.requestAnimationFrame(() => {
      const content = contentRef.current;
      if (content && !content.contains(document.activeElement)) {
        (initialFocusRef?.current ?? content).focus({ preventScroll: true });
      }
    });

    return () => {
      window.cancelAnimationFrame(frame);
      const target = returnFocusRef?.current ?? previousFocus;
      if (target?.isConnected) target.focus({ preventScroll: true });
    };
  }, [open, initialFocusRef, returnFocusRef]);

  if (!mounted) return null;

  return createPortal(
    <div
      ref={backdropRef}
      role="dialog"
      aria-modal="true"
      aria-label={title || "对话框"}
      className={`fixed inset-0 z-50 flex items-center justify-center ${closing ? "modal-backdrop-exit" : "modal-backdrop"}`}
      style={{ background: "rgba(18,24,34,0.08)" }}
      onClick={(e) => {
        if (dismissible && open && e.target === backdropRef.current) onClose();
      }}
      onContextMenu={(e) => {
        if (!dismissible || !closeOnContextMenu) return;
        e.preventDefault();
        if (open) onClose();
      }}
    >
      <div
        ref={contentRef}
        tabIndex={-1}
        className={`modal-surface relative w-full ${width} mx-4 rounded-modal overflow-hidden focus:outline-none ${closing ? "modal-content-exit" : "modal-content"}`}
        style={{
          background: "rgb(var(--surface-card-rgb))",
          border: "1px solid var(--border-default)",
          boxShadow: "var(--shadow-elevated)",
        }}
      >
        {/* Header */}
        {title && (
          <div className="modal-header flex items-center justify-between gap-4 px-5 pt-5 pb-3">
            <h2 className="text-sm font-semibold text-text-primary tracking-normal">{title}</h2>
            {dismissible && (
              <button
                type="button"
                onClick={onClose}
                aria-label="关闭对话框"
                className="icon-btn h-[32px] w-[32px] shrink-0 hover:!bg-surface-hover"
              >
                <X size={14} />
              </button>
            )}
          </div>
        )}

        {/* Body */}
        <div className={`modal-body px-5 ${footer ? "pb-3" : "pb-5"} ${!title ? "pt-5" : ""}`}>
          {children}
        </div>

        {/* Footer */}
        {footer && (
          <div className="modal-footer px-5 pb-5 pt-2 flex justify-end gap-2">
            {footer}
          </div>
        )}
      </div>
    </div>,
    document.body,
  );
}
