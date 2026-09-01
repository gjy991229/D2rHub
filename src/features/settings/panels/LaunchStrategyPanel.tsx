import { Toggle } from "../../../components/ui/Toggle";
import type { GlobalConfig } from "../../../store/types";

interface LaunchStrategyPanelProps {
  config: GlobalConfig;
  updateConfig: (updater: (config: GlobalConfig) => void) => void;
}
export function LaunchStrategyPanel({ config, updateConfig }: LaunchStrategyPanelProps) {
  return (
    <div className="settings-content-grid">
      <section className="spatial-panel p-3 space-y-2" aria-labelledby="agent-strategy-title">
        <div>
          <h2 id="agent-strategy-title" className="text-sm font-semibold text-text-secondary block mb-1">多开 Agent 限制行为</h2>
          <p className="text-2xs text-text-muted">防止启动多个战网客户端冲突的控制策略</p>
        </div>

        <div className="flex gap-2" role="radiogroup" aria-label="Agent 限制行为">
          {[1, 2, 3].map(mode => {
            const label = mode === 1 ? "模式1：延时杀" : mode === 2 ? "模式2：进程数杀" : "模式3：不处理";
            const active = (config.agent_mode ?? 1) === mode;
            return (
              <button
                type="button"
                role="radio"
                aria-checked={active}
                key={mode}
                onClick={() => updateConfig(c => { c.agent_mode = mode; })}
                className={`flex-1 px-3 py-2 rounded-lg text-xs font-semibold transition-all duration-150 border ${
                  active
                    ? "border-accent bg-accent/10 text-accent font-bold"
                    : "border-border-default text-text-secondary hover:text-text-primary"
                }`}
              >
                {label}
              </button>
            );
          })}
        </div>

        {(config.agent_mode ?? 1) === 1 && (
          <div className="border-t border-border-default/50 pt-3 space-y-2">
            <span className="text-xs text-text-muted font-medium block">检测到 Agent 后延迟杀死（秒）</span>
            <div className="flex items-center gap-3">
              <input
                aria-label="Agent 终止延迟"
                type="range"
                min={0}
                max={30}
                step={0.1}
                value={config.agent_delay_secs ?? 1}
                onChange={e => updateConfig(c => { c.agent_delay_secs = parseFloat(parseFloat(e.target.value).toFixed(1)); })}
                className="flex-1 h-1.5 rounded-full appearance-none bg-surface-hover cursor-pointer"
              />
              <span className="text-xs font-mono text-text-primary w-12 text-right font-bold">
                {(config.agent_delay_secs ?? 1).toFixed(1)}s
              </span>
            </div>
            <p className="text-2xs text-text-muted">在此时间内继续进行后续挂载或登录行为而不阻塞。默认 1.0s，范围 0-30s，最小粒度 0.1s</p>
          </div>
        )}

        {(config.agent_mode ?? 1) === 2 && (
          <div className="border-t border-border-default/50 pt-3 space-y-2">
            <span className="text-xs text-text-muted font-medium block">战网客户端运行数阈值</span>
            <div className="flex gap-2" role="radiogroup" aria-label="战网客户端运行数阈值">
              {[5, 7].map(n => {
                const active = (config.agent_threshold ?? 5) === n;
                return (
                  <button
                    type="button"
                    role="radio"
                    aria-checked={active}
                    key={n}
                    onClick={() => updateConfig(c => { c.agent_threshold = n; })}
                    className={`flex-1 px-3 py-1.5 rounded-lg text-xs font-medium transition-all duration-150 border ${
                      active
                        ? "border-accent bg-accent/10 text-accent font-bold"
                        : "border-border-default text-text-secondary hover:text-text-primary"
                    }`}
                  >
                    ≥ {n} 运行客户端
                  </button>
                );
              })}
            </div>
            <p className="text-2xs text-text-muted">仅当活跃战网进程数达到或超过阈值时才终结 Agent，避免多开限制发生。</p>
          </div>
        )}
      </section>

      <section className="spatial-panel p-3 space-y-2" aria-labelledby="application-options-title">
        <h2 id="application-options-title" className="text-xs font-bold text-text-primary">应用行为</h2>
        <div className="flex items-center justify-between py-1.5">
          <div>
            <span className="text-sm font-semibold text-text-secondary">自动关闭隔离浏览器</span>
            <p className="text-2xs text-text-muted">在账号挂载或登录完自动关闭浏览器以释放内存</p>
          </div>
          <Toggle checked={!!config.auto_close_browser} onChange={v => updateConfig(c => { c.auto_close_browser = v; })} />
        </div>
        <div className="flex items-center justify-between py-1.5 border-t border-border-default/50 pt-2">
          <div>
            <span className="text-sm font-semibold text-text-secondary">每日检查更新</span>
            <p className="text-2xs text-text-muted">每天第一次启动多开工具时自动检测新版本</p>
          </div>
          <Toggle checked={!!config.enable_auto_update} onChange={v => updateConfig(c => { c.enable_auto_update = v; })} />
        </div>
      </section>
    </div>
  );
}
