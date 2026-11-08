import { useRef, useState } from "react";
import { ArrowRight, Cat, Monitor, Plus, Route, ScanEye, type LucideIcon } from "lucide-react";
import { Button } from "../../../components/ui/Button";
import { showToast } from "../../../components/ui/Toast";
import type { GlobalConfig } from "../../../store/types";
import { SETTINGS_COPY, normalizeSettingsLanguage, type OptionalModuleTabId } from "../settingsRegistry";
import "../extensions.css";

interface ModuleManagementPanelProps {
  config: GlobalConfig;
  installedModules: readonly OptionalModuleTabId[];
  onInstall: (module: OptionalModuleTabId) => Promise<void> | void;
  onUninstall: (module: OptionalModuleTabId) => Promise<void> | void;
  onOpen: (module: OptionalModuleTabId) => void;
}

const MODULES: readonly { id: OptionalModuleTabId; icon: LucideIcon; benefit: string; enBenefit: string }[] = [
  { id: "overlays", icon: Monitor, benefit: "在游戏旁查看恐怖区域和本局统计。", enBenefit: "Keep Terror Zones and run statistics beside your game." },
  { id: "automation", icon: ScanEye, benefit: "自动识别场景和掉落，记录每一轮刷图。", enBenefit: "Recognize scenes and drops, and keep a history of your runs." },
  { id: "room-automation", icon: Route, benefit: "主账号创建房间，其他账号按顺序跟随。", enBenefit: "Create a room on the primary account and bring the others along." },
  { id: "pet", icon: Cat, benefit: "用桌面角色呈现输入反馈与运行状态。", enBenefit: "A desktop companion with input reactions and status feedback." },
];

export function ModuleManagementPanel({ config, installedModules, onInstall, onUninstall, onOpen }: ModuleManagementPanelProps) {
  const [busyModule, setBusyModule] = useState<OptionalModuleTabId | null>(null);
  const changing = useRef(false);
  const [confirmModule, setConfirmModule] = useState<OptionalModuleTabId | null>(null);
  const language = normalizeSettingsLanguage(config.app_language);
  const en = language === "en-US";

  const changeInstallation = async (module: OptionalModuleTabId, install: boolean) => {
    if (changing.current) return;
    changing.current = true;
    setBusyModule(module);
    try {
      if (install) await onInstall(module);
      else await onUninstall(module);
      setConfirmModule(null);
    } catch (error) {
      showToast("error", en ? `Unable to change this extension: ${error}` : `无法更新扩展功能：${error}`);
    } finally {
      changing.current = false;
      setBusyModule(null);
    }
  };

  const removalDescription = (module: OptionalModuleTabId) => {
    if (module === "overlays" && installedModules.includes("automation")) {
      return en
        ? "Recognition & Stats requires Desktop Overlays and will also be removed. Their running features will stop."
        : "识别与统计需要桌面悬浮窗，会一并移除；相关运行功能将停止。";
    }
    if (module === "automation") {
      return en
        ? "Recognition and statistics will stop. The independent Terror Zone window keeps its current setting."
        : "识别与场景统计将停止，独立的恐怖区域窗口保持当前设置。";
    }
    return en ? "This feature will stop. Your preferences are kept for later." : "此功能将停止运行，已有配置会保留。";
  };

  return (
    <div className="extensions-overview">
      <header className="extensions-heading">
        <h2>{SETTINGS_COPY[language]["module-management"].label}</h2>
        <p>{en ? "Add the tools you need. Their settings appear in the sidebar." : "添加需要的工具，随后直接从左侧进入设置。"}</p>
      </header>
      <section className="extensions-list" aria-label={en ? "Available extensions" : "可用扩展"}>
        {MODULES.map(({ id, icon: Icon, benefit, enBenefit }) => {
          const installed = installedModules.includes(id);
          const name = SETTINGS_COPY[language][id].label;
          const confirming = confirmModule === id;
          return (
            <article key={id} className="extension-row" aria-label={name} data-added={installed} aria-busy={busyModule === id || undefined}>
              <Icon className="extension-icon" size={21} aria-hidden="true" />
              <div className="extension-copy">
                <div className="extension-title"><h3>{name}</h3><span>{installed ? (en ? "Added" : "已添加") : (en ? "Not added" : "未添加")}</span></div>
                <p>{en ? enBenefit : benefit}</p>
                {!installed && id === "automation" && <small>{en
                  ? "Also adds Desktop Overlays and turns on statistics and Terror Zone windows."
                  : "同时添加桌面悬浮窗，并开启统计与恐怖区域窗口。"}</small>}
                {!installed && id === "overlays" && <small>{en
                  ? "Starts with the Terror Zone window."
                  : "添加后默认开启恐怖区域窗口。"}</small>}
              </div>
              <div className="extension-actions">
                {installed ? <>
                  <Button size="sm" variant="secondary" disabled={busyModule !== null} onClick={() => onOpen(id)} aria-label={en ? `Configure ${name}` : `设置${name}`}>
                    {en ? "Configure" : "设置"}<ArrowRight size={13} aria-hidden="true" />
                  </Button>
                  <button type="button" className="extension-remove" disabled={busyModule !== null} aria-expanded={confirming}
                    aria-label={en ? `Remove ${name}` : `移除${name}`} onClick={() => setConfirmModule(confirming ? null : id)}>
                    {en ? "Remove" : "移除"}
                  </button>
                </> : <Button size="sm" variant="secondary" loading={busyModule === id} disabled={busyModule !== null}
                    aria-label={en ? `Add ${name}` : `添加${name}`} onClick={() => void changeInstallation(id, true)}>
                  {busyModule !== id && <Plus size={13} aria-hidden="true" />}{busyModule === id ? (en ? "Adding…" : "正在添加…") : (en ? "Add" : "添加")}
                </Button>}
              </div>
              {confirming && <div className="extension-confirm" role="alert">
                <p>{removalDescription(id)}</p>
                <div>
                  <Button size="sm" variant="ghost" disabled={busyModule !== null} onClick={() => setConfirmModule(null)}>{en ? "Cancel" : "取消"}</Button>
                  <Button size="sm" variant="danger" loading={busyModule === id} disabled={busyModule !== null} onClick={() => void changeInstallation(id, false)}>{busyModule === id ? (en ? "Removing…" : "正在移除…") : (en ? "Confirm removal" : "确认移除")}</Button>
                </div>
              </div>}
            </article>
          );
        })}
      </section>
      <p className="extensions-footnote">{en ? "Removing an extension stops its features and keeps your preferences." : "移除扩展会停止相关功能，保留已有配置。"}</p>
    </div>
  );
}
