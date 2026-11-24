import type { WindowLayout } from "../../store/types";
import { useId } from "react";
import { visibleSlotRect } from "../../features/windowLayouts/model";

export function SchemeLayoutPreview({ layout, names }: { layout: WindowLayout; names: string[] }) {
  const previewId = useId().replaceAll(":", "");
  if (!layout.monitors.length) return null;
  const left = Math.min(...layout.monitors.map(monitor => monitor.bounds.x));
  const top = Math.min(...layout.monitors.map(monitor => monitor.bounds.y));
  const width = Math.max(...layout.monitors.map(monitor => monitor.bounds.x + monitor.bounds.width)) - left;
  const height = Math.max(...layout.monitors.map(monitor => monitor.bounds.y + monitor.bounds.height)) - top;
  if (width <= 0 || height <= 0) return null;
  return <svg className="scheme-layout-preview" role="img" aria-label="方案窗口位置预览" viewBox={`${left - 30} ${top - 30} ${width + 60} ${height + 60}`}>
    <defs>{layout.windows.map((slot, index) => {
      const rect = visibleSlotRect(slot, layout.monitors);
      return <clipPath id={`${previewId}-${index}`} key={index}><rect x={rect.x + 8} y={rect.y + 8} width={Math.max(0, rect.width - 16)} height={Math.max(0, rect.height - 16)} /></clipPath>;
    })}</defs>
    {layout.monitors.map(monitor => <rect key={monitor.id} x={monitor.bounds.x} y={monitor.bounds.y} width={monitor.bounds.width} height={monitor.bounds.height} fill="var(--surface-base)" stroke="var(--border-strong)" vectorEffect="non-scaling-stroke"/>)}
    {layout.windows.map((slot, index) => {
      if (!names[index]) return null;
      const { x, y, width, height } = visibleSlotRect(slot, layout.monitors);
      return <g key={index}><title>{`${index + 1}. ${names[index]} · ${width}×${height}`}</title>
        <rect x={x} y={y} width={width} height={height} fill="var(--surface-active)" fillOpacity={0.8} stroke="var(--text-secondary)" vectorEffect="non-scaling-stroke"/>
        <text x={x + 24} y={y + 60} fill="var(--text-primary)" fontSize={Math.max(28, width / 65)} clipPath={`url(#${previewId}-${index})`}>{index + 1}. {names[index]}</text>
      </g>;
    }).reverse()}
  </svg>;
}
