import type { WindowLayout } from "../../store/types";
import { useId } from "react";

export function SchemeLayoutPreview({ layout, names }: { layout: WindowLayout; names: string[] }) {
  const previewId = useId().replaceAll(":", "");
  if (!layout.monitors.length) return null;
  const left = Math.min(...layout.monitors.map(monitor => monitor.work_area.x));
  const top = Math.min(...layout.monitors.map(monitor => monitor.work_area.y));
  const width = Math.max(...layout.monitors.map(monitor => monitor.work_area.x + monitor.work_area.width)) - left;
  const height = Math.max(...layout.monitors.map(monitor => monitor.work_area.y + monitor.work_area.height)) - top;
  if (width <= 0 || height <= 0) return null;
  return <svg className="scheme-layout-preview" role="img" aria-label="方案窗口位置预览" viewBox={`${left - 30} ${top - 30} ${width + 60} ${height + 60}`}>
    <defs>{layout.windows.map((slot, index) => {
      const monitor = layout.monitors.find(monitor => monitor.id === slot.monitor_id);
      return monitor && <clipPath id={`${previewId}-${index}`} key={index}><rect x={monitor.work_area.x + slot.x + 8} y={monitor.work_area.y + slot.y + 8} width={Math.max(0, slot.width - 16)} height={Math.max(0, slot.height - 16)} /></clipPath>;
    })}</defs>
    {layout.monitors.map(monitor => <rect key={monitor.id} x={monitor.work_area.x} y={monitor.work_area.y} width={monitor.work_area.width} height={monitor.work_area.height} fill="var(--surface-base)" stroke="var(--border-strong)" vectorEffect="non-scaling-stroke"/>)}
    {layout.windows.map((slot, index) => {
      const monitor = layout.monitors.find(monitor => monitor.id === slot.monitor_id);
      if (!monitor || !names[index]) return null;
      const x = monitor.work_area.x + slot.x, y = monitor.work_area.y + slot.y;
      return <g key={index}><title>{`${index + 1}. ${names[index]} · ${slot.width}×${slot.height}`}</title>
        <rect x={x} y={y} width={slot.width} height={slot.height} fill="var(--surface-active)" fillOpacity={0.8} stroke="var(--text-secondary)" vectorEffect="non-scaling-stroke"/>
        <text x={x + 24} y={y + 60} fill="var(--text-primary)" fontSize={Math.max(28, width / 65)} clipPath={`url(#${previewId}-${index})`}>{index + 1}. {names[index]}</text>
      </g>;
    }).reverse()}
  </svg>;
}
