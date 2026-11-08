import { useEffect, useId, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";

interface Props {
  open: boolean;
  anchor: RefObject<HTMLElement | null>;
  title: string;
  onClose: () => void;
  children: ReactNode;
  className?: string;
  width?: number;
  closeLabel?: string;
}

/** Non-modal, anchored content: preserves the workspace geometry and reading order. */
export function AnchoredPanel({ open, anchor, title, onClose, children, className = "", width = 440, closeLabel = "关闭" }: Props) {
  const panel = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  const titleId = useId();
  const [position, setPosition] = useState({ left: 16, top: 16 });
  const [mounted, setMounted] = useState(open);

  useEffect(() => {
    if (open) { setMounted(true); return; }
    if (!mounted) return;
    const timeout = window.setTimeout(() => setMounted(false), 100);
    return () => window.clearTimeout(timeout);
  }, [open, mounted]);

  useLayoutEffect(() => {
    if (!open) return;
    const place = () => {
      const trigger = anchor.current;
      const content = panel.current;
      if (!trigger || !content) return;
      const rect = trigger.getBoundingClientRect();
      if (rect.bottom < 0 || rect.top > window.innerHeight) { closeRef.current(); return; }
      const box = content.getBoundingClientRect();
      const below = window.innerHeight - rect.bottom - 24;
      const above = rect.top - 24;
      const top = below >= box.height || below >= above ? rect.bottom + 8 : rect.top - box.height - 8;
      setPosition({
        left: Math.max(16, Math.min(rect.left, window.innerWidth - box.width - 16)),
        top: Math.max(16, Math.min(top, window.innerHeight - box.height - 16)),
      });
    };
    place();
    const observer = typeof ResizeObserver !== "undefined" ? new ResizeObserver(place) : null;
    if (panel.current) observer?.observe(panel.current);
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
  }, [open, anchor, width]);

  useEffect(() => {
    if (!open) return;
    const content = panel.current;
    const trigger = anchor.current;
    const frame = requestAnimationFrame(() => {
      if (content && !content.contains(document.activeElement)) {
        content.focus({ preventScroll: true });
      }
    });
    const outside = (event: PointerEvent) => {
      if (event.target instanceof Node && !content?.contains(event.target) && !trigger?.contains(event.target)) closeRef.current();
    };
    const focus = (event: FocusEvent) => {
      if (event.target instanceof Node && !content?.contains(event.target) && !trigger?.contains(event.target)) closeRef.current();
    };
    document.addEventListener("pointerdown", outside, true);
    document.addEventListener("focusin", focus);
    return () => {
      cancelAnimationFrame(frame);
      document.removeEventListener("pointerdown", outside, true);
      document.removeEventListener("focusin", focus);
      if ((content?.contains(document.activeElement) || document.activeElement === document.body) && trigger?.isConnected) trigger.focus({ preventScroll: true });
    };
  }, [open, anchor]);

  if (!open && !mounted) return null;
  return createPortal(
    <div ref={panel} role="dialog" aria-labelledby={titleId} tabIndex={-1} aria-hidden={!open} inert={!open}
      className={`anchored-panel ${open ? "" : "anchored-panel-closing"} ${className}`}
      style={{ ...position, width, maxWidth: "calc(100vw - 32px)", maxHeight: "calc(100vh - 32px)" }}
      onClick={event => event.stopPropagation()} onPointerDown={event => event.stopPropagation()}
      onKeyDown={event => {
        if (event.key === "Escape" && !event.defaultPrevented) {
          event.preventDefault(); event.stopPropagation(); closeRef.current();
        }
      }}>
      <header className="anchored-panel-heading">
        <h2 id={titleId}>{title}</h2>
        <button type="button" aria-label={closeLabel} onClick={onClose}><X size={15} /></button>
      </header>
      {children}
    </div>, document.body,
  );
}
