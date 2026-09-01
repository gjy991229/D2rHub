import { Download, FileArchive, FolderOpen, Settings, ShieldAlert, Upload } from "lucide-react";
import type { Dispatch, SetStateAction } from "react";
import { Button } from "../../../components/ui/Button";
import type { AccountMeta } from "../../../store/types";

interface MaintenancePanelProps {
  accounts: AccountMeta[];
  transferBusy: "export" | "import" | null;
  exportPickerOpen: boolean;
  setExportPickerOpen: Dispatch<SetStateAction<boolean>>;
  exportAccountIds: string[];
  setExportAccountIds: Dispatch<SetStateAction<string[]>>;
  plaintextRiskAcknowledged: boolean;
  setPlaintextRiskAcknowledged: Dispatch<SetStateAction<boolean>>;
  onToggleExportAccount: (accountId: string) => void;
  onExport: () => void | Promise<void>;
  onImport: () => void | Promise<void>;
  onOpenLogs: () => void | Promise<void>;
  diagnosticBusy: boolean;
  onExportDiagnostics: () => void | Promise<void>;
  onRunSetup: () => void;
}
export function MaintenancePanel({
  accounts,
  transferBusy,
  exportPickerOpen,
  setExportPickerOpen,
  exportAccountIds,
  setExportAccountIds,
  plaintextRiskAcknowledged,
  setPlaintextRiskAcknowledged,
  onToggleExportAccount,
  onExport,
  onImport,
  onOpenLogs,
  diagnosticBusy,
  onExportDiagnostics,
  onRunSetup,
}: MaintenancePanelProps) {
  return (
    <div className="settings-content-grid">
      <section className="spatial-panel p-3 space-y-2 settings-span-full" aria-labelledby="maintenance-title">
        <h2 id="maintenance-title" className="text-xs font-bold text-text-primary">应用维护</h2>
        <div className="flex items-center justify-between gap-3 py-1">
          <div>
            <span className="text-sm font-semibold text-text-secondary">打开系统运行日志</span>
            <p className="text-2xs text-text-muted">查看当前多开工具的底层系统日志以供排查故障</p>
          </div>
          <Button size="sm" onClick={onOpenLogs}>
            <FolderOpen size={11} className="mr-1" />
            打开日志
          </Button>
        </div>

        <div className="flex items-center justify-between gap-3 py-1 border-t border-border-default/50 pt-3">
          <div>
            <span className="text-sm font-semibold text-text-secondary">导出隐私脱敏诊断包</span>
            <p className="text-2xs text-text-muted">包含模块健康、任务时间线、配置摘要与脱敏日志，不导出 Token、账号名或用户目录</p>
          </div>
          <Button size="sm" loading={diagnosticBusy} onClick={onExportDiagnostics}>
            <FileArchive size={11} className="mr-1" />
            {diagnosticBusy ? "正在导出" : "导出诊断包"}
          </Button>
        </div>

        <div className="flex items-center justify-between gap-3 py-1 border-t border-border-default/50 pt-3">
          <div>
            <span className="text-sm font-semibold text-text-secondary">重新配置游戏路径</span>
            <p className="text-2xs text-text-muted">重新运行首次引导向导以修改基础路径配置</p>
          </div>
          <Button size="sm" onClick={onRunSetup}>
            <Settings size={11} className="mr-1" />
            运行向导
          </Button>
        </div>
      </section>

      <section className="spatial-panel p-3 space-y-3 settings-span-full" aria-labelledby="account-transfer-title">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="max-w-[68ch]">
            <h2 id="account-transfer-title" className="text-xs font-bold text-text-primary">账号迁移</h2>
            <p className="text-2xs text-text-muted mt-1 leading-relaxed">
              导出账号元数据、独立画质配置与认证快照，不包含浏览器缓存。Token 会解密后以明文写入 JSON，导入时再使用目标设备的 Windows DPAPI 加密。
            </p>
          </div>
          <div className="flex gap-2">
            <Button
              size="sm"
              disabled={accounts.length === 0 || transferBusy !== null}
              onClick={() => {
                setExportAccountIds(accounts.map(account => account.id));
                setPlaintextRiskAcknowledged(false);
                setExportPickerOpen(current => !current);
              }}
            >
              <Download size={11} className="mr-1" />
              导出账号
            </Button>
            <Button size="sm" disabled={transferBusy !== null} onClick={onImport}>
              <Upload size={11} className="mr-1" />
              {transferBusy === "import" ? "导入中" : "导入账号"}
            </Button>
          </div>
        </div>

        {exportPickerOpen && (
          <div className="border-t border-border-default/50 pt-3 space-y-3">
            <div
              role="alert"
              className="flex items-start gap-2 rounded-lg px-3 py-2.5"
              style={{ background: "var(--toast-warning-bg)", border: "1px solid var(--toast-warning-border)" }}
            >
              <ShieldAlert size={14} className="text-warning shrink-0 mt-0.5" />
              <div className="min-w-0">
                <p className="text-xs font-semibold text-text-primary">导出文件包含明文登录凭据</p>
                <p className="text-2xs text-text-secondary mt-1 leading-relaxed max-w-[72ch]">
                  任何获得这份 JSON 的人都可以使用其中的 Token 登录你的账号。请只保存到可信位置，不要发送给其他人；迁移完成后应立即安全删除。
                </p>
              </div>
            </div>
            <div className="flex items-center justify-between gap-3">
              <p className="text-xs font-semibold text-text-secondary">
                选择要写入导出文件的账号 · 已选 {exportAccountIds.length}/{accounts.length}
              </p>
              <button
                type="button"
                className="text-xs text-accent hover:underline"
                onClick={() => setExportAccountIds(
                  exportAccountIds.length === accounts.length ? [] : accounts.map(account => account.id),
                )}
              >
                {exportAccountIds.length === accounts.length ? "取消全选" : "全选"}
              </button>
            </div>
            <div className="grid grid-cols-2 gap-2 max-[720px]:grid-cols-1">
              {accounts.map(account => {
                const selected = exportAccountIds.includes(account.id);
                return (
                  <label
                    key={account.id}
                    className="option-line min-w-0 cursor-pointer rounded-lg px-2.5 py-2"
                    style={{ background: selected ? "var(--surface-hover)" : "transparent" }}
                  >
                    <input
                      type="checkbox"
                      className="sr-only"
                      checked={selected}
                      onChange={() => onToggleExportAccount(account.id)}
                    />
                    <span className={selected ? "check-box checked" : "check-box"} />
                    <span className="truncate text-xs text-text-secondary">
                      {account.display_name || account.id}
                    </span>
                  </label>
                );
              })}
            </div>
            <label className="option-line min-h-7 h-auto cursor-pointer rounded-lg px-2.5 py-2 bg-surface-hover">
              <input
                type="checkbox"
                className="sr-only"
                checked={plaintextRiskAcknowledged}
                onChange={(event) => setPlaintextRiskAcknowledged(event.target.checked)}
              />
              <span className={plaintextRiskAcknowledged ? "check-box checked" : "check-box"} />
              <span className="text-xs text-text-secondary leading-relaxed">
                我已理解导出文件包含可直接使用的登录凭据，并会妥善保管
              </span>
            </label>
            <div className="flex justify-end gap-2">
              <Button
                size="sm"
                onClick={() => {
                  setExportPickerOpen(false);
                  setPlaintextRiskAcknowledged(false);
                }}
              >
                取消
              </Button>
              <Button
                variant="primary"
                size="sm"
                disabled={exportAccountIds.length === 0 || !plaintextRiskAcknowledged || transferBusy !== null}
                onClick={onExport}
              >
                {transferBusy === "export" ? "导出中" : `导出 ${exportAccountIds.length} 个账号（含明文）`}
              </Button>
            </div>
          </div>
        )}
      </section>
    </div>
  );
}
