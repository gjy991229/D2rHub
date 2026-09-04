import { useEffect, useMemo, useState } from "react";
import { Check, FolderOpen, PackageOpen, Plus, RefreshCw, RotateCcw, Trash2, X } from "lucide-react";

import { Button } from "../../../components/ui/Button";
import { Modal } from "../../../components/ui/Modal";
import { Toggle } from "../../../components/ui/Toggle";
import { showToast } from "../../../components/ui/Toast";
import type { AccountMeta, ModCapsule } from "../../../store/types";
import type { ModCapsuleController } from "../../modCapsules/useModCapsulePool";
import { capsuleBaseModLabel, capsuleFeatureLabels } from "../../modCapsules/model";

interface ModCatalogManagerProps {
  catalog: ModCapsuleController;
  accounts: AccountMeta[];
  autoOpenAdd?: boolean;
  initialEdition?: string;
  language?: string | null;
  onProcess: (capsule: ModCapsule) => Promise<void> | void;
}

const COPY = {
  "zh-CN": {
    title: "Mod 管理", description: "游戏目录中的 Mod 自动成为共享预设；账号、声纹识别和自动跟房都从这里选择同一份配置。",
    scan: "扫描目录", openFolder: "打开文件夹", openFolderTitle: "打开当前版本的 mods 文件夹", editions: "游戏版本", cn: "国服", global: "国际服", add: "添加自定义参数",
    addLabel: "添加自定义共享参数", customTitle: "自定义共享参数",
    customHelp: "用于保留旧账号或特殊启动写法；普通 Mod 会由目录扫描自动加入。",
    cancel: "取消", save: "保存", addSuccess: "自定义参数已加入 Mod 列表",
    scanned: "游戏目录预设 · 名称由文件夹决定", custom: "自定义共享参数", baseMod: "基 Mod",
    credentialsPending: "凭证待更新", inUse: "使用中",
    updateSuccess: "共享参数已更新，引用它的账号和启动方案已同步",
    autoExit: "死亡自动退房", autoExitOn: "已启用；关闭游戏后可在此停用", autoExitOff: "已停用；关闭游戏后可在此启用",
    autoExitEnabled: "死亡自动退房已启用，重新启动游戏后生效", autoExitDisabled: "死亡自动退房已停用，重新启动游戏后生效",
    process: "加工", editTitle: "点击编辑启动参数", restoreTitle: "恢复官方预设", restore: "恢复",
    restoreSuccess: "已恢复标准启动参数", delete: "删除",
    deleteScannedTitle: (name: string) => `删除 Mod“${name}”？`,
    deleteScannedDescription: (name: string) => `删除此参数会同时永久删除游戏目录中的 Mod 文件夹“mods\\${name}”。此操作不可撤销。`,
    deleteScannedAction: "删除参数和 Mod", deleteScannedSuccess: (name: string) => `Mod“${name}”及其参数已删除`,
    deleteCustomTitle: "删除自定义参数？",
    deleteCustomDescription: "只会删除这条自定义参数，不会删除或修改任何 Mod 文件。",
    deleteCustomAction: "删除参数", deleteCustomSuccess: "自定义参数已删除",
    empty: (value: string) => `没有扫描到 ${value === "CN" ? "国服" : "国际服"} Mod`,
    emptyHelp: "请确认游戏目录下存在 mods\\Mod名\\Mod名.mpq，然后重新扫描。",
    footnote: "点击参数条可编辑；扫描 Mod 的 -mod 名称必须与文件夹名称一致。删除扫描 Mod 会同时删除对应文件夹。",
  },
  "en-US": {
    title: "Mod Management", description: "Mods in the game directory become shared presets. Accounts, recognition, and room automation all use this catalog.",
    scan: "Scan folders", openFolder: "Open folder", openFolderTitle: "Open the mods folder for this game edition", editions: "Game edition", cn: "China", global: "Global", add: "Add custom arguments",
    addLabel: "Add shared custom arguments", customTitle: "Shared custom arguments",
    customHelp: "Keep legacy or specialized launch syntax here. Regular Mods are discovered from the game directory.",
    cancel: "Cancel", save: "Save", addSuccess: "Custom arguments added to the Mod list",
    scanned: "Game-directory preset · folder name is authoritative", custom: "Shared custom arguments", baseMod: "Base Mod",
    credentialsPending: "Metadata update required", inUse: "Used by",
    updateSuccess: "Shared arguments updated across accounts and launch schemes",
    autoExit: "Auto-exit on death", autoExitOn: "On; close the game before turning it off here", autoExitOff: "Off; close the game before turning it on here",
    autoExitEnabled: "Auto-exit on death enabled; restart the game to apply", autoExitDisabled: "Auto-exit on death disabled; restart the game to apply",
    process: "Process", editTitle: "Click to edit launch arguments", restoreTitle: "Restore default preset", restore: "Restore",
    restoreSuccess: "Default launch arguments restored", delete: "Delete",
    deleteScannedTitle: (name: string) => `Delete “${name}”?`,
    deleteScannedDescription: (name: string) => `Deleting these arguments will also permanently delete the corresponding Mod folder, “mods\\${name}”. This cannot be undone.`,
    deleteScannedAction: "Delete arguments and Mod", deleteScannedSuccess: (name: string) => `“${name}” and its arguments were deleted`,
    deleteCustomTitle: "Delete custom arguments?",
    deleteCustomDescription: "Only these custom arguments will be deleted. No Mod files will be deleted or changed.",
    deleteCustomAction: "Delete arguments", deleteCustomSuccess: "Custom arguments deleted",
    empty: (value: string) => `No ${value === "CN" ? "China" : "Global"} Mods found`,
    emptyHelp: "Confirm that mods\\ModName\\ModName.mpq exists in the game directory, then scan again.",
    footnote: "Click the arguments strip to edit it. A scanned Mod must keep the -mod name that matches its folder. Deleting a scanned Mod also deletes that folder.",
  },
} as const;

export function ModCatalogManager({ catalog, accounts, autoOpenAdd, initialEdition, language, onProcess }: ModCatalogManagerProps) {
  const isEnglish = language === "en-US";
  const copy = COPY[isEnglish ? "en-US" : "zh-CN"];
  const [edition, setEdition] = useState<"CN" | "Global">(
    catalog.pool?.capsules[0]?.edition === "Global" ? "Global" : "CN",
  );
  const [editingId, setEditingId] = useState<string | null>(null);
  const [argumentDraft, setArgumentDraft] = useState("");
  const [addOpen, setAddOpen] = useState(false);
  const [addDraft, setAddDraft] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<ModCapsule | null>(null);
  const capsules = useMemo(
    () => (catalog.pool?.capsules ?? []).filter((capsule) => capsule.edition === edition),
    [catalog.pool, edition],
  );

  useEffect(() => {
    if (autoOpenAdd) {
      setAddOpen(true);
      if (initialEdition === "CN" || initialEdition === "Global") setEdition(initialEdition);
    }
  }, [autoOpenAdd, initialEdition]);

  useEffect(() => {
    const all = catalog.pool?.capsules ?? [];
    if (all.length && !all.some((capsule) => capsule.edition === edition)) {
      setEdition(all.some((capsule) => capsule.edition === "CN") ? "CN" : "Global");
    }
  }, [catalog.pool, edition]);

  const run = async (operation: () => Promise<unknown>, success: string) => {
    try {
      await operation();
      showToast("success", success);
      return true;
    } catch (error) {
      showToast("error", String(error));
      return false;
    }
  };

  const beginEdit = (capsule: ModCapsule) => {
    setEditingId(capsule.id);
    setArgumentDraft(capsule.launch_arguments);
    setDeleteTarget(null);
  };

  const deleteIsScanned = deleteTarget?.origin === "scanned";
  const deleteTitle = deleteTarget
    ? deleteIsScanned
      ? copy.deleteScannedTitle(deleteTarget.name)
      : copy.deleteCustomTitle
    : "";
  const deleteDescription = deleteTarget
    ? deleteIsScanned
      ? copy.deleteScannedDescription(deleteTarget.name)
      : copy.deleteCustomDescription
    : "";
  const closeDeleteConfirmation = () => {
    if (!catalog.loading) setDeleteTarget(null);
  };

  return (
    <div className="mod-catalog-manager">
      <header className="mod-processing-header">
        <div>
          <h2>{copy.title}</h2>
          <p>{copy.description}</p>
        </div>
        <div className="mod-processing-header-actions">
          <Button
            size="sm"
            variant="ghost"
            title={copy.openFolderTitle}
            onClick={() => void catalog.openDirectory?.(edition).catch((error) => {
              showToast("error", String(error));
            })}
          >
            <FolderOpen size={13} />{copy.openFolder}
          </Button>
          <Button size="sm" variant="ghost" loading={catalog.loading} onClick={() => void catalog.scan()}>
            <RefreshCw size={13} />{copy.scan}
          </Button>
        </div>
      </header>

      <div className="mod-catalog-toolbar">
        <div className="mod-catalog-editions" role="tablist" aria-label={copy.editions}>
          {(["CN", "Global"] as const).map((value) => (
            <button key={value} type="button" role="tab" aria-selected={edition === value} onClick={() => setEdition(value)}>
              {value === "CN" ? copy.cn : copy.global}
            </button>
          ))}
        </div>
        <Button size="sm" variant="secondary" onClick={() => { setAddOpen(true); setAddDraft(""); }}>
          <Plus size={13} />{copy.add}
        </Button>
      </div>

      {addOpen && (
        <section className="mod-catalog-add" aria-label={copy.addLabel}>
          <div>
            <strong>{copy.customTitle}</strong>
            <small>{copy.customHelp}</small>
          </div>
          <input
            className="settings-input"
            value={addDraft}
            autoFocus
            placeholder={isEnglish ? "Example: -mod MyMod -txt -assettestmode 1" : "例如：-mod MyMod -txt -assettestmode 1"}
            onChange={(event) => setAddDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") setAddOpen(false);
              if (event.key === "Enter" && addDraft.trim()) {
                void run(() => catalog.add(edition, addDraft.trim()), copy.addSuccess).then((saved) => {
                  if (saved) { setAddOpen(false); setAddDraft(""); }
                });
              }
            }}
          />
          <div>
            <Button size="sm" variant="ghost" onClick={() => setAddOpen(false)}><X size={12} />{copy.cancel}</Button>
            <Button
              size="sm"
              variant="primary"
              disabled={!addDraft.trim() || catalog.loading}
              onClick={() => void run(() => catalog.add(edition, addDraft.trim()), copy.addSuccess).then((saved) => {
                if (saved) { setAddOpen(false); setAddDraft(""); }
              })}
            ><Check size={12} />{copy.save}</Button>
          </div>
        </section>
      )}

      {catalog.error && <p className="mod-catalog-error" role="status">{catalog.error}</p>}
      <div className="mod-catalog-list" aria-busy={catalog.loading}>
        {capsules.map((capsule) => {
          const editing = editingId === capsule.id;
          const assignedNames = capsule.assigned_account_ids
            .map((id) => accounts.find((account) => account.id === id)?.display_name || id)
            .join(isEnglish ? ", " : "、");
          const featureLabels = capsuleFeatureLabels(capsule, isEnglish);
          const supportsAutoExitOnDeath = capsule.origin === "scanned"
            && capsule.feature_groups.includes("auto_exit_on_death");
          const autoExitDescriptionId = `mod-auto-exit-${capsule.id.replace(/[^a-zA-Z0-9_-]/g, "-")}`;
          return (
            <article key={capsule.id} className="mod-catalog-row" data-processed={capsule.processed ? "true" : undefined} data-assigned={assignedNames ? "true" : undefined}>
              <div className="mod-catalog-identity">
                <span className="mod-catalog-capsule"><PackageOpen size={13} /><b>{capsule.name}</b></span>
                <small>{capsule.origin === "scanned" ? copy.scanned : copy.custom}</small>
                <div className="mod-catalog-state">
                  {capsule.processed && (
                    <div className="mod-catalog-capability-capsules">
                      <span data-kind="base" title={copy.baseMod}>{capsuleBaseModLabel(capsule, isEnglish)}</span>
                      {featureLabels.length
                        ? featureLabels.map((label) => <span data-kind="feature" key={label}>{label}</span>)
                        : <span data-kind="pending">{copy.credentialsPending}</span>}
                    </div>
                  )}
                  {!!assignedNames && <small title={assignedNames}>{copy.inUse}: {assignedNames}</small>}
                </div>
              </div>
              {editing ? (
                <div className="mod-catalog-editor">
                  <input
                    className="settings-input"
                    value={argumentDraft}
                    autoFocus
                    onChange={(event) => setArgumentDraft(event.target.value)}
                    onKeyDown={(event) => { if (event.key === "Escape") setEditingId(null); }}
                  />
                  <Button size="sm" variant="primary" disabled={!argumentDraft.trim()} onClick={() => void run(
                    () => catalog.update(capsule.id, argumentDraft.trim()),
                    copy.updateSuccess,
                  ).then((saved) => { if (saved) setEditingId(null); })}><Check size={12} />{copy.save}</Button>
                  <Button size="sm" variant="ghost" onClick={() => setEditingId(null)}><X size={12} />{copy.cancel}</Button>
                </div>
              ) : (
                <div className="mod-catalog-configuration">
                  <button
                    type="button"
                    className="mod-catalog-arguments"
                    title={`${copy.editTitle}: ${capsule.launch_arguments}`}
                    aria-label={`${copy.editTitle}: ${capsule.name}`}
                    disabled={catalog.loading}
                    onClick={() => beginEdit(capsule)}
                  >
                    <code>{capsule.launch_arguments}</code>
                  </button>
                  {supportsAutoExitOnDeath && (
                    <div className="mod-catalog-feature-control">
                      <span>
                        <b>{copy.autoExit}</b>
                        <small id={autoExitDescriptionId}>
                          {capsule.auto_exit_on_death_enabled
                            ? copy.autoExitOn
                            : copy.autoExitOff}
                        </small>
                      </span>
                      <Toggle
                        checked={capsule.auto_exit_on_death_enabled === true}
                        disabled={catalog.loading}
                        ariaLabel={`${capsule.name} ${copy.autoExit}`}
                        descriptionId={autoExitDescriptionId}
                        onChange={(enabled) => void run(
                          () => catalog.setAutoExitOnDeathEnabled(capsule.id, enabled),
                          enabled ? copy.autoExitEnabled : copy.autoExitDisabled,
                        )}
                      />
                    </div>
                  )}
                </div>
              )}
              {!editing && (
                <div className="mod-catalog-actions">
                  {capsule.origin === "scanned" && (
                    capsule.source_eligible || capsule.update_required || capsule.processed
                  ) && (
                    <Button size="sm" variant="ghost" onClick={() => void onProcess(capsule)}>{copy.process}</Button>
                  )}
                  {capsule.deletable && (
                    <Button
                      size="sm"
                      variant="ghost"
                      title={capsule.origin === "scanned" ? copy.deleteScannedAction : copy.deleteCustomAction}
                      onClick={() => { setEditingId(null); setDeleteTarget(capsule); }}
                    >
                      <Trash2 size={12} />{copy.delete}
                    </Button>
                  )}
                  {capsule.origin === "scanned" && capsule.default_launch_arguments
                    && capsule.launch_arguments !== capsule.default_launch_arguments && (
                      <Button size="sm" variant="ghost" title={copy.restoreTitle} onClick={() => void run(
                        () => catalog.update(capsule.id, capsule.default_launch_arguments!),
                        copy.restoreSuccess,
                      )}><RotateCcw size={12} />{copy.restore}</Button>
                    )}
                </div>
              )}
            </article>
          );
        })}
        {!catalog.loading && capsules.length === 0 && (
          <div className="mod-catalog-empty">
            <PackageOpen size={22} />
            <strong>{copy.empty(edition)}</strong>
            <p>{copy.emptyHelp}</p>
          </div>
        )}
      </div>
      <footer className="mod-catalog-footnote">
        {copy.footnote}
      </footer>
      <Modal
        open={deleteTarget !== null}
        onClose={closeDeleteConfirmation}
        title={deleteTitle}
        width="max-w-sm"
        dismissible={!catalog.loading}
        footer={(
          <>
            <Button autoFocus size="sm" variant="secondary" disabled={catalog.loading} onClick={closeDeleteConfirmation}>
              {copy.cancel}
            </Button>
            <Button
              size="sm"
              variant="danger"
              loading={catalog.loading}
              onClick={() => {
                if (!deleteTarget) return;
                const target = deleteTarget;
                const success = target.origin === "scanned"
                  ? copy.deleteScannedSuccess(target.name)
                  : copy.deleteCustomSuccess;
                void run(() => catalog.remove(target.id), success).then((removed) => {
                  if (removed) setDeleteTarget(null);
                });
              }}
            >
              <Trash2 size={12} />
              {deleteIsScanned ? copy.deleteScannedAction : copy.deleteCustomAction}
            </Button>
          </>
        )}
      >
        <p className="mod-catalog-delete-description">{deleteDescription}</p>
      </Modal>
    </div>
  );
}
