import "./progressBar.css";

export function ProgressBar({ value, label }: { value?: number; label: string }) {
  const progress = value === undefined ? undefined : Math.max(0, Math.min(100, value));
  return <div className="hub-progress" role="progressbar" aria-label={label}
    aria-valuemin={0} aria-valuemax={100} aria-valuenow={progress} data-indeterminate={progress === undefined}>
    <span style={progress === undefined ? undefined : { width: `${progress}%` }} />
  </div>;
}
