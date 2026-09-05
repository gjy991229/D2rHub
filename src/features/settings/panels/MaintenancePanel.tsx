import { Download, FileArchive, FolderOpen, Layers3, Minimize2, Settings, ShieldAlert, Upload } from "lucide-react";
import { useState, type Dispatch, type SetStateAction } from "react";
import { Button } from "../../../components/ui/Button";
import { Modal } from "../../../components/ui/Modal";
import type { FeatureProfile } from "../../profile/featureProfile";
import type { AccountMeta } from "../../../store/types";

interface MaintenancePanelProps {
  featureProfile: FeatureProfile;
  profileChanging: boolean;
  installedOptionalModuleCount: number;
  onChangeFeatureProfile: (profile: FeatureProfile) => Promise<boolean>;
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
  featureProfile,
  profileChanging,
  installedOptionalModuleCount,
  onChangeFeatureProfile,
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
  const [pendingProfile, setPendingProfile] = useState<FeatureProfile | null>(null);
  const switchingToMinimal = pendingProfile === "minimal";

  return (
    <>
    <div className="settings-content-grid">
      <section className="spatial-panel p-3 space-y-3 settings-span-full" aria-labelledby="feature-profile-title">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="max-w-[68ch]">
            <h2 id="feature-profile-title" className="text-xs font-bold text-text-primary">使用模式</h2>
            <p className="text-2xs text-text-muted mt-1 leading-relaxed">
              极简模式只显示多开、Mod 管理和必要设置。切换不会卸载模块，也不会删除模块配置。
            </p>
          </div>
          <span className="settings-navigation-badge" data-state="configured">
            当前：{featureProfile === "minimal" ? "极简模式" : "正常模式"}
          </span>
        </div>
        <div className="grid grid-cols-2 gap-2 max-[620px]:grid-cols-1" role="radiogroup" aria-label="D2RHub 使用模式">
          {([
            ["minimal", "极简模式", "只保留多开与 Mod 管理", Minimize2],
            ["normal", "正常模式", "显示完整功能并恢复原模块状态", Layers3],
          ] as const).map(([value, label, detail, Icon]) => {
            const selected = featureProfile === value;
            return (
              <button
                key={value}
                type="button"
                role="radio"
                aria-checked={selected}
                disabled={selected || profileChanging}
                onClick={() => setPendingProfile(value)}
                className="flex min-h-[64px] items-center gap-3 rounded-card px-3.5 py-3 text-left transition-all focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-2 disabled:cursor-default"
                style={{
                  background: selected ? "rgb(var(--accent-rgb) / 0.08)" : "var(--surface-hover)",
                  border: selected ? "1px solid rgb(var(--accent-rgb) / 0.3)" : "1px solid var(--border-default)",
                }}
              >
                <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-[11px] border border-border-default bg-surface-card text-text-secondary">
                  <Icon size={15} aria-hidden="true" />
                </span>
                <span className="min-w-0">
                  <strong className="block text-xs text-text-primary">{label}</strong>
                  <small className="mt-0.5 block text-2xs leading-relaxed text-text-muted">{detail}</small>
                </span>
              </button>
            );
          })}
        </div>
      </section>

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
    <Modal
      open={pendingProfile !== null}
      onClose={() => { if (!profileChanging) setPendingProfile(null); }}
      title={switchingToMinimal ? "切换到极简模式？" : "切换到正常模式？"}
      width="max-w-sm"
      dismissible={!profileChanging}
      footer={(
        <div className="flex justify-end gap-2">
          <Button size="sm" disabled={profileChanging} onClick={() => setPendingProfile(null)}>取消</Button>
          <Button
            size="sm"
            variant="primary"
            loading={profileChanging}
            onClick={() => {
              if (!pendingProfile) return;
              void onChangeFeatureProfile(pendingProfile).then((changed) => {
                if (changed) setPendingProfile(null);
              });
            }}
          >
            确认切换
          </Button>
        </div>
      )}
    >
      <div className="space-y-2 text-sm leading-relaxed text-text-secondary">
        <p>{switchingToMinimal
          ? `将暂停并隐藏其他功能模块${installedOptionalModuleCount > 0 ? `（现有 ${installedOptionalModuleCount} 个模块配置会完整保留）` : ""}。多开、Mod 管理和正在运行的游戏不受影响。`
          : `将恢复完整界面${installedOptionalModuleCount > 0 ? `，并按原设置恢复 ${installedOptionalModuleCount} 个已安装模块` : ""}。单个模块如启动失败，不会影响多开与 Mod 管理。`}</p>
        <p className="text-xs text-text-muted">切换完成后仍可在本页改回另一种模式。</p>
        {switchingToMinimal && (
          <p className="text-xs text-text-muted">会等待附加任务与悬浮窗停止后再完成切换；停止失败会保留原模式。已取消的自动跟房任务不会自动重跑。</p>
        )}
      </div>
    </Modal>
    </>
  );
}
