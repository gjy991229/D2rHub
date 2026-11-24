import { useEffect, useId, useRef, useState, type CSSProperties, type PointerEvent } from "react";
import type { LayoutRect, LayoutSlot, WindowLayout } from "../../store/types";
import { moveSlot, resizeSlot, slotRect, visibleSlotRect, visibleInsets, type ResizeHandle } from "./model";

const handles: ResizeHandle[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];

export function LayoutCanvas({ layout, labels, selected, onSelect, onChange, focusedMonitor, english, disabled }: {
  layout: WindowLayout;
  labels: string[];
  selected: number;
  onSelect: (index: number) => void;
  onChange: (windows: LayoutSlot[]) => void;
  focusedMonitor?: string;
  english?: boolean;
  disabled?: boolean;
}) {
  const svg = useRef<SVGSVGElement>(null);
  const clipId = useId();
  const [viewport, setViewport] = useState({ width: 760, height: 360 });
  const [dragging, setDragging] = useState(false);
  const frame = useRef(0);
  const pending = useRef<LayoutSlot[] | null>(null);
  const drag = useRef<{ index: number; pointer: number; x: number; y: number; layout: WindowLayout; handle?: ResizeHandle } | null>(null);
  const displayed = focusedMonitor ? layout.monitors.filter(monitor => monitor.id === focusedMonitor) : layout.monitors;
  const left = Math.min(...displayed.map(monitor => monitor.bounds.x));
  const top = Math.min(...displayed.map(monitor => monitor.bounds.y));
  const right = Math.max(...displayed.map(monitor => monitor.bounds.x + monitor.bounds.width));
  const bottom = Math.max(...displayed.map(monitor => monitor.bounds.y + monitor.bounds.height));
  const sceneScale = Math.min((viewport.width - 32) / Math.max(1, right - left), (viewport.height - 60) / Math.max(1, bottom - top));
  const bounds: LayoutRect = { x: left - 16 / sceneScale, y: top - 42 / sceneScale,
    width: right - left + 32 / sceneScale, height: bottom - top + 60 / sceneScale };
  const scale = Math.min(viewport.width / bounds.width, viewport.height / bounds.height);

  useEffect(() => {
    const element = svg.current;
    if (!element) return;
    const measure = () => {
      const box = element.getBoundingClientRect();
      if (box.width > 0 && box.height > 0) setViewport({ width: box.width, height: box.height });
    };
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  useEffect(() => () => { if (frame.current) cancelAnimationFrame(frame.current); }, []);

  const point = (event: PointerEvent) => {
    const box = svg.current!.getBoundingClientRect();
    const actualScale = Math.min((box.width || viewport.width) / bounds.width, (box.height || viewport.height) / bounds.height);
    return {
      x: bounds.x + (event.clientX - box.left - ((box.width || viewport.width) - bounds.width * actualScale) / 2) / actualScale,
      y: bounds.y + (event.clientY - box.top - ((box.height || viewport.height) - bounds.height * actualScale) / 2) / actualScale,
    };
  };
  const begin = (event: PointerEvent, index: number, handle?: ResizeHandle) => {
    if (event.button !== 0 || disabled) return;
    event.preventDefault();
    event.stopPropagation();
    (event.currentTarget.closest("g[role='button']") as SVGElement | null)?.focus({ preventScroll: true });
    const start = point(event);
    onSelect(index);
    if (layout.windows[index].fullscreen) return;
    drag.current = { index, pointer: event.pointerId, ...start, layout, handle };
    setDragging(true);
    svg.current?.setPointerCapture(event.pointerId);
  };
  const flush = () => {
    frame.current = 0;
    if (pending.current) { onChange(pending.current); pending.current = null; }
  };
  const move = (event: PointerEvent<SVGSVGElement>) => {
    const start = drag.current;
    if (!start || start.pointer !== event.pointerId) return;
    const current = point(event);
    const dx = current.x - start.x, dy = current.y - start.y;
    const initial = start.layout.windows[start.index];
    const tolerance = event.shiftKey ? 0 : 7 / scale;
    const rect = slotRect(initial, start.layout.monitors);
    const next = start.handle ? resizeSlot(start.layout, start.index, initial, dx, dy, start.handle, tolerance)
      : moveSlot(start.layout, start.index, { ...rect, x: rect.x + dx, y: rect.y + dy }, tolerance);
    pending.current = start.layout.windows.map((slot, index) => index === start.index ? next : slot);
    if (!frame.current) frame.current = requestAnimationFrame(flush);
  };
  const end = (event: PointerEvent<SVGSVGElement>, cancelled = false) => {
    if (drag.current?.pointer !== event.pointerId) return;
    if (frame.current) cancelAnimationFrame(frame.current);
    frame.current = 0;
    if (cancelled) { pending.current = null; onChange(drag.current.layout.windows); } else flush();
    drag.current = null;
    setDragging(false);
    if (svg.current?.hasPointerCapture(event.pointerId)) svg.current.releasePointerCapture(event.pointerId);
  };
  const visibleSlots = layout.windows.map((slot, index) => ({ slot, index })).filter(({ slot }) => !focusedMonitor || slot.monitor_id === focusedMonitor);
  // Paint the centered primary last, then the selected window. Every slot is
  // also reachable in the account list even when completely covered.
  visibleSlots.sort((a, b) => (a.index === selected ? 2 : a.index === 0 ? 1 : 0) - (b.index === selected ? 2 : b.index === 0 ? 1 : 0) || b.index - a.index);
  return <svg ref={svg} className="layout-canvas" viewBox={`${bounds.x} ${bounds.y} ${bounds.width} ${bounds.height}`}
    style={{ "--canvas-scale": scale } as CSSProperties} role="group" aria-label={english ? "Window layout canvas" : "窗口布局画布"}
    data-dragging={dragging} onPointerMove={move} onPointerUp={event => end(event)} onPointerCancel={event => end(event, true)}
    onLostPointerCapture={event => { if (drag.current) end(event, true); }}>
    {displayed.map(monitor => <g key={monitor.id} className="layout-monitor">
      <rect className="layout-monitor-bounds" x={monitor.bounds.x} y={monitor.bounds.y} width={monitor.bounds.width} height={monitor.bounds.height} rx={8 / scale} vectorEffect="non-scaling-stroke" />
      <rect className="layout-monitor-work" x={monitor.work_area.x} y={monitor.work_area.y} width={monitor.work_area.width} height={monitor.work_area.height} rx={6 / scale} />
      <text className="layout-monitor-label" x={monitor.bounds.x + 12 / scale} y={monitor.bounds.y - 16 / scale}>
        {monitor.name}{monitor.primary ? (english ? " · Primary" : " · 主屏") : ""} · {monitor.bounds.width} × {monitor.bounds.height} · {Math.round(monitor.scale_factor * 100)}%
      </text>
    </g>)}
    {visibleSlots.map(({ slot, index }) => {
      const rect = visibleSlotRect(slot, layout.monitors);
      const edge = visibleInsets(layout.monitors.find(monitor => monitor.id === slot.monitor_id), slot.fullscreen);
      const client = slotRect(slot, layout.monitors);
      const active = index === selected;
      const padding = 12 / scale;
      const handleSize = 9 / scale;
      return <g key={index} className="layout-window" data-selected={active} data-primary={index === 0} role="button" tabIndex={disabled ? -1 : 0} aria-disabled={disabled || undefined}
        aria-label={english ? `${labels[index]}, ${client.width} by ${client.height}` : `${labels[index]}，${client.width} × ${client.height}`}
        aria-pressed={active} onFocus={() => onSelect(index)} onPointerDown={event => begin(event, index)}
        onKeyDown={event => {
          if (disabled) return;
          if (["Enter", " "].includes(event.key)) { event.preventDefault(); onSelect(index); return; }
          if (event.key === "Escape" && drag.current) {
            event.preventDefault(); event.stopPropagation();
            if (frame.current) cancelAnimationFrame(frame.current);
            frame.current = 0; pending.current = null; onChange(drag.current.layout.windows); drag.current = null; setDragging(false);
            return;
          }
          if (slot.fullscreen || !["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) return;
          event.preventDefault();
          const delta = event.ctrlKey || event.metaKey ? 1 : 10;
          const dx = event.key === "ArrowLeft" ? -delta : event.key === "ArrowRight" ? delta : 0;
          const dy = event.key === "ArrowUp" ? -delta : event.key === "ArrowDown" ? delta : 0;
          const next = event.shiftKey ? resizeSlot(layout, index, slot, dx, dy, "se")
            : moveSlot(layout, index, { ...slotRect(slot, layout.monitors), x: rect.x + dx, y: rect.y + dy });
          onChange(layout.windows.map((candidate, at) => at === index ? next : candidate));
        }}>
        <rect className="layout-window-body" x={rect.x} y={rect.y} width={rect.width} height={rect.height} rx={slot.fullscreen ? 0 : 5 / scale} vectorEffect="non-scaling-stroke" />
        {edge.top > 0 && <g className="layout-window-caption" aria-hidden="true">
          <rect x={rect.x} y={rect.y} width={rect.width} height={edge.top} />
          <path d={`M ${rect.x + rect.width - edge.top * 0.8} ${rect.y + edge.top * 0.35} l ${edge.top * 0.3} ${edge.top * 0.3} m 0 ${-edge.top * 0.3} l ${-edge.top * 0.3} ${edge.top * 0.3}`} />
        </g>}
        <rect className="layout-window-client" x={rect.x + edge.left} y={rect.y + edge.top} width={client.width} height={client.height} />
        <title>{labels[index]} · {english ? "Position" : "位置"} {index + 1}</title>
        <defs><clipPath id={`${clipId}-${index}`}><rect x={rect.x + padding} y={rect.y} width={Math.max(0, rect.width - 2 * padding)} height={rect.height} /></clipPath></defs>
        <text className="layout-window-name" x={rect.x + padding} y={rect.y + edge.top + 27 / scale} clipPath={`url(#${clipId}-${index})`}>{labels[index]}</text>
        {rect.width * scale >= 110 && rect.height * scale >= 65 && <text className="layout-window-size" x={rect.x + padding} y={rect.y + edge.top + 45 / scale}>{client.width} × {client.height}</text>}
        {active && !slot.fullscreen && handles.map(handle => {
          const x = handle.includes("w") ? rect.x : handle.includes("e") ? rect.x + rect.width : rect.x + rect.width / 2;
          const y = handle.includes("n") ? rect.y : handle.includes("s") ? rect.y + rect.height : rect.y + rect.height / 2;
          return <rect key={handle} className={`layout-resize-handle layout-resize-${handle}`} x={x - handleSize / 2} y={y - handleSize / 2}
            width={handleSize} height={handleSize} rx={2 / scale} vectorEffect="non-scaling-stroke" onPointerDown={event => begin(event, index, handle)} />;
        })}
      </g>;
    })}
  </svg>;
}
