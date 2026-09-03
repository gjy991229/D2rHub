import { useState } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { showToast } from "../../components/ui/Toast";
import { invokeCommand } from "../../platform/tauri";
import type { AccountMeta } from "../../store/types";

interface ExportAccountsSummary {
  account_count: number;
  plaintext_token_count: number;
}

interface ImportAccountsSummary {
  imported: { id: string; display_name: string; initialized: boolean }[];
  warnings: string[];
  reencrypted_token_count: number;
}

export function useMaintenanceController(
  accounts: AccountMeta[],
  loadAccounts: () => Promise<void>,
) {
  const [exportPickerOpen, setExportPickerOpen] = useState(false);
  const [exportAccountIds, setExportAccountIds] = useState<string[]>([]);
  const [plaintextRiskAcknowledged, setPlaintextRiskAcknowledged] = useState(false);
  const [transferBusy, setTransferBusy] = useState<"export" | "import" | null>(null);
  const [diagnosticBusy, setDiagnosticBusy] = useState(false);

  const toggleExportAccount = (accountId: string) => {
    setExportAccountIds((current) => current.includes(accountId)
      ? current.filter((id) => id !== accountId)
      : [...current, accountId]);
  };

  const exportAccounts = async () => {
    if (exportAccountIds.length === 0) {
      showToast("warning", "请至少选择一个要导出的账号");
      return;
    }
    if (!plaintextRiskAcknowledged) {
      showToast("warning", "请先确认已理解明文 Token 的账号安全风险");
      return;
    }
    const date = new Date().toISOString().slice(0, 10);
    const destination = await saveDialog({
      title: "导出 D2RHub 账号",
      defaultPath: `D2RHub-accounts-${date}.json`,
      filters: [{ name: "D2RHub 账号包", extensions: ["json"] }],
    });
    if (!destination) return;
    setTransferBusy("export");
    try {
      const summary = await invokeCommand<ExportAccountsSummary>("export_accounts", {
        accountIds: exportAccountIds,
        destination,
        acknowledgePlaintextRisk: true,
      });
      showToast("success", `已导出 ${summary.account_count} 个账号`);
      showToast(
        "warning",
        summary.plaintext_token_count > 0
          ? `导出文件包含 ${summary.plaintext_token_count} 个明文 Token；请妥善保管并在迁移后删除`
          : "导出文件仍包含账号认证快照，请妥善保管并在迁移后删除",
      );
      setExportPickerOpen(false);
      setPlaintextRiskAcknowledged(false);
    } catch (error) {
      showToast("error", `导出账号失败: ${error}`);
    } finally {
      setTransferBusy(null);
    }
  };

  const importAccounts = async () => {
    const source = await openDialog({
      title: "选择 D2RHub 账号导出文件",
      multiple: false,
      filters: [{ name: "D2RHub 账号包", extensions: ["json"] }],
    });
    if (!source || Array.isArray(source)) return;
    setTransferBusy("import");
    try {
      const summary = await invokeCommand<ImportAccountsSummary>("import_accounts", { source });
      await loadAccounts();
      showToast(
        "success",
        `已导入 ${summary.imported.length} 个账号，本机重新加密 ${summary.reencrypted_token_count} 个 Token`,
      );
      if (summary.warnings.length > 0) {
        const extra = summary.warnings.length > 1 ? `（另有 ${summary.warnings.length - 1} 项提示）` : "";
        showToast("warning", `${summary.warnings[0]}${extra}`);
      }
    } catch (error) {
      showToast("error", `导入账号失败: ${error}`);
    } finally {
      setTransferBusy(null);
    }
  };

  const openLogs = async () => {
    try {
      await invokeCommand("open_logs_dir");
    } catch (error) {
      showToast("error", `打开日志失败: ${error}`);
    }
  };

  const exportDiagnostics = async () => {
    setDiagnosticBusy(true);
    try {
      const path = await invokeCommand<string>("export_diagnostic_bundle");
      showToast("success", `隐私脱敏诊断包已保存：${path}`);
    } catch (error) {
      showToast("error", `导出诊断包失败: ${error}`);
    } finally {
      setDiagnosticBusy(false);
    }
  };

  return {
    accounts,
    exportPickerOpen,
    setExportPickerOpen,
    exportAccountIds,
    setExportAccountIds,
    plaintextRiskAcknowledged,
    setPlaintextRiskAcknowledged,
    transferBusy,
    diagnosticBusy,
    toggleExportAccount,
    exportAccounts,
    importAccounts,
    openLogs,
    exportDiagnostics,
  };
}
