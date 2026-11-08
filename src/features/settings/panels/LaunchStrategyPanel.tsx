import { Toggle } from "../../../components/ui/Toggle";
import { RangeSlider } from "../../../components/ui/RangeSlider";
import type { AccountMeta, GlobalConfig } from "../../../store/types";
import { sortAccountsByCardOrder } from "../../../utils/accountOrder";
import { requiresTokenMigration } from "../../../utils/regionPaths";
import "./corePanels.css";

interface LaunchStrategyPanelProps {
  config: GlobalConfig;
  accounts: AccountMeta[];
  updateConfig: (updater: (config: GlobalConfig) => void) => void;
}
export function LaunchStrategyPanel({ config, accounts, updateConfig }: LaunchStrategyPanelProps) {
  const en = config.app_language === "en-US";
  const battleNetAccounts = sortAccountsByCardOrder(accounts).filter(account => {
    const authMode = account.auth_mode?.trim() || "bnet";
    return account.initialized && authMode === "bnet"
      && !requiresTokenMigration(authMode, account.region, config);
  });
  const configuredAccountId = config.keep_battle_net_account_id?.trim() ?? "";
  const selectedAccountId = battleNetAccounts.find(account => account.id.toLowerCase() === configuredAccountId.toLowerCase())?.id
    ?? configuredAccountId;
  const selectedAccountUnavailable = !!selectedAccountId
    && !battleNetAccounts.some(account => account.id === selectedAccountId);

  return (
    <div className="settings-content-grid">


      <section className="spatial-panel p-3 space-y-2" aria-labelledby="keep-battle-net-title">
        <div>
          <h2 id="keep-battle-net-title" className="text-sm font-semibold text-text-secondary mb-1">多开后保留战网</h2>
          <p id="keep-battle-net-description" className="text-2xs text-text-muted">所选账号参与本次启动且全部账号启动成功后，单独重新打开它的战网客户端，不重复启动游戏；取消或启动失败时跳过。</p>
        </div>
        <label className="block text-xs text-text-secondary" htmlFor="keep-battle-net-account">保留战网的账号</label>
        <select
          id="keep-battle-net-account"
          className="settings-input"
          aria-describedby="keep-battle-net-description keep-battle-net-status"
          value={selectedAccountId}
          onChange={event => updateConfig(c => { c.keep_battle_net_account_id = event.target.value || null; })}
        >
          <option value="">不保留</option>
          {selectedAccountUnavailable && <option value={selectedAccountId} disabled>原账号不可用，请重新选择</option>}
          {battleNetAccounts.map(account => (
            <option key={account.id} value={account.id} data-i18n-skip>{account.display_name || account.id}</option>
          ))}
        </select>
        <p id="keep-battle-net-status" className={`text-2xs ${selectedAccountUnavailable ? "text-warning" : "text-text-muted"}`}>
          {selectedAccountUnavailable
            ? "原账号已删除、未初始化或已改为 Token 模式，请重新选择。"
            : battleNetAccounts.length === 0
              ? "暂无可用的战网模式账号，请先添加并初始化战网模式账号。"
              : "仅可选择已初始化的战网模式账号。"}
        </p>
      </section>

      <section className="spatial-panel p-3 space-y-2" aria-labelledby="application-options-title">
        <h2 id="application-options-title" className="text-xs font-bold text-text-primary">应用行为</h2>
        <div className="flex items-center justify-between gap-3 py-1.5">
          <div>
            <span className="text-sm font-semibold text-text-secondary">启动时自动整理内存</span>
            <p id="launch-memory-description" className="text-2xs text-text-muted">成功启动数量达到一半（向上取整）和批次结束时，整理已成功启动窗口的内存；单窗口仅整理一次，后续加载可能短暂卡顿。</p>
          </div>
          <Toggle
            checked={config.launch_trim_memory ?? true}
            ariaLabel="启动时自动整理内存"
            descriptionId="launch-memory-description"
            onChange={v => updateConfig(c => { c.launch_trim_memory = v; })}
          />
        </div>
        <div className="flex items-center justify-between gap-3 py-1.5">
          <div>
            <span className="text-sm font-semibold text-text-secondary">自动关闭隔离浏览器</span>
            <p className="text-2xs text-text-muted">在账号挂载或登录完自动关闭浏览器以释放内存</p>
          </div>
          <Toggle checked={!!config.auto_close_browser} ariaLabel="自动关闭隔离浏览器" onChange={v => updateConfig(c => { c.auto_close_browser = v; })} />
        </div>
        <div className="flex items-center justify-between gap-3 py-1.5 border-t border-border-default/50 pt-2">
          <div>
            <span className="text-sm font-semibold text-text-secondary">每日检查更新</span>
            <p className="text-2xs text-text-muted">每天首次启动检查软件、加工器和已安装Mod；仅提示，不自动安装</p>
          </div>
          <Toggle checked={!!config.enable_auto_update} ariaLabel="每日检查更新" onChange={v => updateConfig(c => { c.enable_auto_update = v; })} />
        </div>
        <label className="flex items-center justify-between gap-3 py-1.5">
          <span className="text-sm font-semibold text-text-secondary">下载源</span>
          <select className="settings-input" style={{ width: "auto" }} value={config.download_source ?? "auto"}
            onChange={e => updateConfig(c => { c.download_source = e.target.value as "auto" | "gitee" | "github"; })}>
            <option value="auto">自动</option><option value="gitee">优先 Gitee</option><option value="github">优先 GitHub</option>
          </select>
        </label>
        <p className="text-2xs text-text-muted">自动模式按网络地区选择；首选源失败时切换备用源，取消下载不会重试。</p>
      </section>
      <details className="spatial-panel p-3 space-y-2" >
        <summary className="settings-advanced-summary">高级启动兼容设置 <span>遇到战网多开问题时调整</span></summary>
        <div>
          <h2 id="agent-strategy-title" className="text-sm font-semibold text-text-secondary block mb-1">多开 Agent 限制行为</h2>
          <p className="text-2xs text-text-muted">防止启动多个战网客户端冲突的控制策略</p>
        </div>

        <div className="launch-agent-options" role="radiogroup" aria-label="Agent 限制行为" data-i18n-skip>
          {[1, 2, 3].map(mode => {
            const label = mode === 1 ? (en ? "End after a delay" : "延时结束 Agent") : mode === 2 ? (en ? "End at client limit" : "按客户端数量结束") : (en ? "Keep Agent running" : "保留 Agent");
            const active = (config.agent_mode ?? 1) === mode;
            return (
              <label
                key={mode}
                className="launch-agent-choice"
                data-selected={active}
              >
                <input className="sr-only" type="radio" name="agent-mode" value={mode} checked={active}
                  onChange={() => updateConfig(c => { c.agent_mode = mode; })} />
                {label}
              </label>
            );
          })}
        </div>

        {(config.agent_mode ?? 1) === 1 && (
          <div className="border-t border-border-default/50 pt-3 space-y-2">
            <span className="text-xs text-text-muted font-medium block" data-i18n-skip>{en ? "Delay before ending Agent (seconds)" : "检测到 Agent 后等待时间（秒）"}</span>
            <div className="flex items-center gap-3">
              <RangeSlider
                aria-label="Agent 终止延迟"
                min={0}
                max={30}
                step={0.1}
                value={config.agent_delay_secs ?? 1}
                onChange={e => updateConfig(c => { c.agent_delay_secs = parseFloat(parseFloat(e.target.value).toFixed(1)); })}
                className="min-w-0 flex-1"
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
            <div className="launch-agent-options launch-agent-threshold" role="radiogroup" aria-label="战网客户端运行数阈值">
              {[5, 7].map(n => {
                const active = (config.agent_threshold ?? 5) === n;
                return (
                  <label
                    key={n}
                    className="launch-agent-choice" data-selected={active}
                  >
                    <input type="radio" name="agent-threshold" className="sr-only" value={n} checked={active}
                      onChange={() => updateConfig(c => { c.agent_threshold = n; })} />
                    ≥ {n} 运行客户端
                  </label>
                );
              })}
            </div>
            <p className="text-2xs text-text-muted">仅当活跃战网进程数达到或超过阈值时才终结 Agent，避免多开限制发生。</p>
          </div>
        )}
      </details>
    </div>
  );
}
