import { useEffect, useMemo, useState } from "react";
import { Check, Edit3, FolderOpen, PackageOpen, Plus, RefreshCw, RotateCcw, Trash2, X } from "lucide-react";

import { Button } from "../../../components/ui/Button";
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
    process: "加工", editTitle: "编辑启动参数", edit: "编辑", restoreTitle: "恢复官方预设", restore: "恢复",
    restoreSuccess: "已恢复标准启动参数", deleteSuccess: "自定义参数已删除", confirmDelete: "确认删除",
    deleteTitle: "删除自定义参数", delete: "删除",
    empty: (value: string) => `没有扫描到 ${value === "CN" ? "国服" : "国际服"} Mod`,
    emptyHelp: "请确认游戏目录下存在 mods\\Mod名\\Mod名.mpq，然后重新扫描。",
    footnote: "扫描预设不能改名或直接删除；删除实体 Mod 文件夹后重新扫描即可移除。账号卡片只负责选择，不会修改 Mod 列表。",
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
    process: "Process", editTitle: "Edit launch arguments", edit: "Edit", restoreTitle: "Restore default preset", restore: "Restore",
    restoreSuccess: "Default launch arguments restored", deleteSuccess: "Custom arguments deleted", confirmDelete: "Confirm delete",
    deleteTitle: "Delete custom arguments", delete: "Delete",
    empty: (value: string) => `No ${value === "CN" ? "China" : "Global"} Mods found`,
    emptyHelp: "Confirm that mods\\ModName\\ModName.mpq exists in the game directory, then scan again.",
    footnote: "Scanned presets cannot be renamed or deleted here. Remove the physical Mod folder and scan again. Account cards only select a Mod; they never edit this list.",
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
  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null);
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
    setDeleteConfirmId(null);
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
                  <code title={capsule.launch_arguments}>{capsule.launch_arguments}</code>
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
                  <Button size="sm" variant="ghost" title={copy.editTitle} onClick={() => beginEdit(capsule)}>
                    <Edit3 size={12} />{copy.edit}
                  </Button>
                  {capsule.origin === "scanned" && capsule.default_launch_arguments
                    && capsule.launch_arguments !== capsule.default_launch_arguments && (
                      <Button size="sm" variant="ghost" title={copy.restoreTitle} onClick={() => void run(
                        () => catalog.update(capsule.id, capsule.default_launch_arguments!),
                        copy.restoreSuccess,
                      )}><RotateCcw size={12} />{copy.restore}</Button>
                    )}
                  {capsule.deletable && (deleteConfirmId === capsule.id ? (
                    <Button size="sm" variant="danger" onClick={() => void run(
                      () => catalog.remove(capsule.id),
                      copy.deleteSuccess,
                    ).then((removed) => { if (removed) setDeleteConfirmId(null); })}>{copy.confirmDelete}</Button>
                  ) : (
                    <Button size="sm" variant="ghost" title={copy.deleteTitle} onClick={() => setDeleteConfirmId(capsule.id)}>
                      <Trash2 size={12} />{copy.delete}
                    </Button>
                  ))}
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
    </div>
  );
}
