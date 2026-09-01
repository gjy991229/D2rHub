import { useEffect, useMemo, useState } from "react";
import { Check, Edit3, PackageOpen, Plus, RefreshCw, RotateCcw, Trash2, X } from "lucide-react";

import { Button } from "../../../components/ui/Button";
import { showToast } from "../../../components/ui/Toast";
import type { AccountMeta, ModCapsule } from "../../../store/types";
import type { ModCapsuleController } from "../../modCapsules/useModCapsulePool";

interface ModCatalogManagerProps {
  catalog: ModCapsuleController;
  accounts: AccountMeta[];
  autoOpenAdd?: boolean;
  initialEdition?: string;
  onProcess: (capsule: ModCapsule) => Promise<void> | void;
}

export function ModCatalogManager({ catalog, accounts, autoOpenAdd, initialEdition, onProcess }: ModCatalogManagerProps) {
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
          <h2>Mod 管理</h2>
          <p>游戏目录中的 Mod 自动成为共享预设；账号、声纹识别和自动跟房都从这里选择同一份配置。</p>
        </div>
        <Button size="sm" variant="ghost" loading={catalog.loading} onClick={() => void catalog.scan()}>
          <RefreshCw size={13} />扫描目录
        </Button>
      </header>

      <div className="mod-catalog-toolbar">
        <div className="mod-catalog-editions" role="tablist" aria-label="游戏版本">
          {(["CN", "Global"] as const).map((value) => (
            <button key={value} type="button" role="tab" aria-selected={edition === value} onClick={() => setEdition(value)}>
              {value === "CN" ? "国服" : "国际服"}
            </button>
          ))}
        </div>
        <Button size="sm" variant="secondary" onClick={() => { setAddOpen(true); setAddDraft(""); }}>
          <Plus size={13} />添加自定义参数
        </Button>
      </div>

      {addOpen && (
        <section className="mod-catalog-add" aria-label="添加自定义共享参数">
          <div>
            <strong>自定义共享参数</strong>
            <small>用于保留旧账号或特殊启动写法；普通 Mod 会由目录扫描自动加入。</small>
          </div>
          <input
            className="settings-input"
            value={addDraft}
            autoFocus
            placeholder="例如：-mod MyMod -txt -assettestmode 1"
            onChange={(event) => setAddDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") setAddOpen(false);
              if (event.key === "Enter" && addDraft.trim()) {
                void run(() => catalog.add(edition, addDraft.trim()), "自定义参数已加入共享池").then((saved) => {
                  if (saved) { setAddOpen(false); setAddDraft(""); }
                });
              }
            }}
          />
          <div>
            <Button size="sm" variant="ghost" onClick={() => setAddOpen(false)}><X size={12} />取消</Button>
            <Button
              size="sm"
              variant="primary"
              disabled={!addDraft.trim() || catalog.loading}
              onClick={() => void run(() => catalog.add(edition, addDraft.trim()), "自定义参数已加入共享池").then((saved) => {
                if (saved) { setAddOpen(false); setAddDraft(""); }
              })}
            ><Check size={12} />保存</Button>
          </div>
        </section>
      )}

      {catalog.error && <p className="mod-catalog-error" role="status">{catalog.error}</p>}
      <div className="mod-catalog-list" aria-busy={catalog.loading}>
        {capsules.map((capsule) => {
          const editing = editingId === capsule.id;
          const assignedNames = capsule.assigned_account_ids
            .map((id) => accounts.find((account) => account.id === id)?.display_name || id)
            .join("、");
          return (
            <article key={capsule.id} className="mod-catalog-row" data-processed={capsule.processed ? "true" : undefined} data-assigned={assignedNames ? "true" : undefined}>
              <div className="mod-catalog-identity">
                <span className="mod-catalog-capsule"><PackageOpen size={13} /><b>{capsule.name}</b></span>
                <small>{capsule.origin === "scanned" ? "游戏目录预设 · 名称由文件夹决定" : "自定义共享参数"}</small>
                <div className="mod-catalog-state">
                  {capsule.processed && <span>已加工</span>}
                  {!!assignedNames && <small title={assignedNames}>使用中：{assignedNames}</small>}
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
                    "共享参数已更新，引用它的账号和启动方案已同步",
                  ).then((saved) => { if (saved) setEditingId(null); })}><Check size={12} />保存</Button>
                  <Button size="sm" variant="ghost" onClick={() => setEditingId(null)}><X size={12} />取消</Button>
                </div>
              ) : (
                <code title={capsule.launch_arguments}>{capsule.launch_arguments}</code>
              )}
              {!editing && (
                <div className="mod-catalog-actions">
                  {capsule.origin === "scanned" && !capsule.processed && (
                    <Button size="sm" variant="ghost" onClick={() => void onProcess(capsule)}>加工</Button>
                  )}
                  <Button size="sm" variant="ghost" title="编辑启动参数" onClick={() => beginEdit(capsule)}>
                    <Edit3 size={12} />编辑
                  </Button>
                  {capsule.origin === "scanned" && capsule.default_launch_arguments
                    && capsule.launch_arguments !== capsule.default_launch_arguments && (
                      <Button size="sm" variant="ghost" title="恢复官方预设" onClick={() => void run(
                        () => catalog.update(capsule.id, capsule.default_launch_arguments!),
                        "已恢复标准启动参数",
                      )}><RotateCcw size={12} />恢复</Button>
                    )}
                  {capsule.deletable && (deleteConfirmId === capsule.id ? (
                    <Button size="sm" variant="danger" onClick={() => void run(
                      () => catalog.remove(capsule.id),
                      "自定义参数已删除",
                    ).then((removed) => { if (removed) setDeleteConfirmId(null); })}>确认删除</Button>
                  ) : (
                    <Button size="sm" variant="ghost" title="删除自定义参数" onClick={() => setDeleteConfirmId(capsule.id)}>
                      <Trash2 size={12} />删除
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
            <strong>没有扫描到 {edition === "CN" ? "国服" : "国际服"} Mod</strong>
            <p>请确认游戏目录下存在 <code>mods\Mod名\Mod名.mpq</code>，然后重新扫描。</p>
          </div>
        )}
      </div>
      <footer className="mod-catalog-footnote">
        扫描预设不能改名或直接删除；删除实体 Mod 文件夹后重新扫描即可移除。账号卡片只负责选择，不会修改共享池。
      </footer>
    </div>
  );
}
