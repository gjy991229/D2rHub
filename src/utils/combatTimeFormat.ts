export function formatCombatTime(milliseconds: number, format = "seconds", english = false) {
  const seconds = Math.max(0, milliseconds) / 1000;
  const whole = Math.floor(seconds);
  const pad = (value: number) => String(value).padStart(2, "0");
  if (format === "hours") {
    return { value: `${Math.floor(whole / 3600)}:${pad(Math.floor(whole / 60) % 60)}:${pad(whole % 60)}`, unit: english ? "H:M:S" : "时:分:秒" };
  }
  if (format === "minutes") {
    return { value: `${pad(Math.floor(whole / 60))}:${pad(whole % 60)}`, unit: english ? "M:S" : "分:秒" };
  }
  return { value: seconds.toFixed(1), unit: english ? "SEC" : "秒" };
}
