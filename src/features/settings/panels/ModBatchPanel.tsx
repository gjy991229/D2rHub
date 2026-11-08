import { ArrowLeft, ArrowRight, CheckCircle2, Layers3, PackageOpen } from "lucide-react";
import { Button } from "../../../components/ui/Button";
import { ProgressBar } from "../../../components/ui/ProgressBar";
import { ModPageHeader } from "./ModPageHeader";
import { RoomToolProcessingOptions } from "./RoomToolProcessingOptions";
import { ModFeatureChoice } from "./ModFeatureChoice";
import type { ModWorkflowController } from "../../mods/workflow/useModWorkflow";
import type { AudioModFeatureSelection } from "../../mods/featureContract";

export function ModBatchPanel({ workflow }: { workflow: ModWorkflowController }) {
  const { batch, en } = workflow;
  const candidates = (batch.inspection?.installed_mods ?? []).filter(mod => (batch.mode === "create" || batch.mode === "replace-source")
    ? mod.source_eligible || mod.requires_unpack : mod.feature_groups.length || mod.update_required);
  const labels = { "replace-source": en ? "Keep source names + backup" : "同名加工并备份", create: en ? "New copies" : "加工新副本", augment: en ? "Add modules" : "增补模块", rebuild: en ? "Update modules" : "模块更新" };
  const states = { blocked: en ? "Blocked" : "需处理", pending: en ? "Pending" : "待执行", running: en ? "Processing" : "处理中", success: en ? "Complete" : "成功", failed: en ? "Failed" : "失败", skipped: en ? "Not started" : "未开始" };
  const featureLabels: [keyof AudioModFeatureSelection, string, string][] = [
    ["includeRoomTools", en ? "In-game room tools" : "局内房间工具", en ? "Create, join, and follow games" : "创建、加入与自动跟房"],
    ["includeHostileAutoRestart", en ? "Hostile auto-restart" : "敌对自动重开", en ? "Requires sound; includes Double Esc" : "需开启声音，自动包含双击 Esc 下一局"],
    ["includePartyLoot", en ? "Player loot buttons" : "左侧玩家尸体拾取按钮", en ? "Native per-player permission controls; no image recognition" : "点击左侧玩家头像旁的原生按钮切换尸体拾取权限，无需图像识别"],
    ["includeSceneShortcuts", en ? "Scene restart" : "场景重开", en ? "Restart or create at an A3, Travincal, or A4 waypoint; no image recognition" : "需在 A3、崔凡克或 A4 小站处重开或创房，无需图像识别"],
    ["includeEscNextGame", en ? "Double Esc: next Hell game" : "双 Esc 下一局地狱", en ? "Press Esc twice to start the next game" : "快速双击 Esc，进入下一局地狱"],
    ["includeAudioTelemetry", en ? "Audio recognition" : "声纹识别", en ? "Drop audio and an OCR scene catalog" : "掉落声纹与 OCR 地区目录"],
    ["includeAutoExitOnDeath", en ? "Exit on death" : "死亡自动退出", en ? "Leave after death; the corpse cannot be recovered in that game" : "死亡后离开房间，本局无法捡回尸体"],
  ];
  const pending = batch.rows?.filter(row => row.state === "pending").length ?? 0;
  const completed = batch.rows?.filter(row => row.state === "success").length ?? 0;
  const description = batch.mode === "create" ? (en ? "Create a separate Mod for each source. Set output names below." : "为每个源 Mod 生成独立副本，在下方分别设置输出名称。")
    : batch.mode === "replace-source" ? (en ? "Verify each result, retain the original folder with a backup suffix, then install under the source name. Close games using these Mods first." : "逐项生成并校验，原目录加 _backup_唯一编号 后缀保留，新 Mod 沿用源名称。请先关闭使用这些 Mod 的游戏。")
    : batch.mode === "augment" ? (en ? "Keep installed modules and add the selected ones. Incompatible protocols trigger a fresh rebuild." : "保留已有模块，增补所选功能；协议不一致时自动从原始源 Mod 重做。")
      : (en ? "Rebuild using each Mod’s original source and saved modules, then replace the same name after verification." : "使用各自的原始来源与已保存模块重新生成，校验后替换同名成品。");

  return <div className="mod-workspace mod-batch-panel">
    <ModPageHeader title={en ? "Batch Mod processing" : "批量加工与更新"}
      description={en ? "Configure, review, then process in order. Choose the results in the Mod library." : "设置加工内容，预览计划，再依次处理。完成后在 Mod 库选用。"}>
      <Button size="sm" variant="ghost" disabled={batch.busy} onClick={() => workflow.actions.openLibrary(batch.edition)}>
        <ArrowLeft size={13} aria-hidden="true" />{en ? "Back to Mod library" : "返回 Mod 库"}
      </Button>
    </ModPageHeader>
    {batch.error && <p role="alert" className="mod-catalog-error">{batch.error}</p>}
    {!batch.rows ? <>
      <section className="mod-processing-section mod-batch-setup">
        <div className="mod-section-heading"><h3>{en ? "Processing setup" : "加工设置"}</h3><p>{en ? "Use the selected account’s directory. Its active Mod stays unchanged." : "使用所选账号的游戏目录，不会自动切换账号正在使用的 Mod。"}</p></div>
        <label className="mod-batch-account"><span>{en ? "Game directory from account" : "使用此账号的游戏目录"}</span>
          <select className="settings-input" value={batch.accountId} disabled={batch.busy || !batch.accounts.length} onChange={event => void batch.chooseAccount(event.target.value)}>
            <option value="" disabled>{en ? "Select an initialized account" : "请选择已初始化账号"}</option>
            {batch.accounts.map(account => <option key={account.id} value={account.id}>{account.display_name || account.id}</option>)}
          </select>
        </label>
        {!batch.accounts.length && <p className="mod-empty-state" role="status">{en ? "Initialize an account for this game edition before processing Mods." : "请先初始化当前游戏版本的账号，再加工 Mod。"}</p>}
        <div className="mod-batch-modes" role="group" aria-label={en ? "Batch operation" : "批量操作"}>
          {(["create", "replace-source", "augment", "rebuild"] as const).map(mode => <button type="button" key={mode} aria-pressed={batch.mode === mode} onClick={() => batch.changeMode(mode)}>{labels[mode]}</button>)}
        </div>
        <p className="mod-section-description">{description}</p>
      </section>
      {batch.mode !== "rebuild" && <fieldset className="mod-processing-section mod-batch-features">
        <legend>{en ? "Modules for this batch" : "本批次添加的模块"}</legend>
        <div className="mod-feature-grid">{featureLabels.map(([key, title, detail]) => <div key={key} className="mod-feature-with-options"><ModFeatureChoice title={title} detail={detail}
          checked={!!batch.features[key] || (key === "includeEscNextGame" && !!batch.features.includeHostileAutoRestart)}
          locked={key === "includeEscNextGame" && !!batch.features.includeHostileAutoRestart}
          lockLabel={en ? "Required by hostile auto-restart" : "敌对自动重开必选"} disabled={batch.busy}
          onChange={checked => batch.changeFeatures({ ...batch.features, [key]: checked,
            ...(key === "includeHostileAutoRestart" && checked ? { includeEscNextGame: true } : {}) })} />
          {key === "includeRoomTools" && batch.features.includeRoomTools && <RoomToolProcessingOptions en={en}
            enabled={batch.features.roomToolsDefaultHell ?? true} disabled={batch.busy}
            onChange={enabled => batch.changeFeatures({ ...batch.features, roomToolsDefaultHell: enabled })} />}
          </div>)}</div>
      </fieldset>}
      <section className="mod-processing-section mod-batch-selection">
        <div className="mod-section-toolbar">
          <div className="mod-section-heading"><h3>{en ? "Select Mods" : "选择 Mod"} <span className="mod-count">{candidates.length}</span></h3></div>
          <div className="mod-page-header-actions"><Button size="sm" variant="ghost" disabled={batch.loading || !candidates.length} onClick={() => batch.select(candidates.filter(mod => !mod.requires_unpack).map(mod => mod.name))}>{en ? "Select available" : "全选可处理项"}</Button>
            <Button size="sm" variant="ghost" disabled={!batch.selected.length} onClick={() => batch.select([])}>{en ? "Clear" : "清空选择"}</Button></div>
        </div>
        {batch.loading ? <p className="mod-empty-state" role="status">{en ? "Inspecting Mods…" : "正在检查 Mod…"}</p> : candidates.length ? <div className="mod-batch-sources">
          <div className="mod-batch-columns" aria-hidden="true"><span>{en ? "Source Mod" : "源 Mod"}</span><span>{batch.mode === "create" ? (en ? "Output name" : "输出名称") : (en ? "Original source" : "原始来源")}</span></div>
          {candidates.map(mod => <div className="mod-batch-source" data-selected={batch.selected.includes(mod.name)} data-disabled={mod.requires_unpack || undefined} key={mod.name}>
            <label><input type="checkbox" checked={batch.selected.includes(mod.name)} disabled={mod.requires_unpack} onChange={event => batch.select(event.target.checked ? [...batch.selected, mod.name] : batch.selected.filter(name => name !== mod.name))} /><PackageOpen size={15} aria-hidden="true" /><strong>{mod.name}</strong></label>
            {mod.requires_unpack ? <span className="mod-section-description">{en ? "Unpack in the library first" : "请先在 Mod 库解压"}</span> : batch.mode === "create"
              ? <input className="settings-input" aria-label={`${mod.name} ${en ? "output name" : "输出名称"}`} value={batch.names[mod.name] ?? `${mod.name}-Hub`} onChange={event => batch.changeName(mod.name, event.target.value)} />
              : batch.mode === "replace-source" ? <small>{en ? "Output: " : "输出："}{mod.name}{en ? " · Original folder retained as backup" : " · 原目录加后缀备份"}</small>
              : <small>{en ? "Source: " : "来源："}{mod.source_mod_name ?? (en ? "Original game / saved recipe" : "原版游戏／旧配方")}{mod.update_required ? (en ? " · Rebuild required" : " · 需要重做") : ""}</small>}
          </div>)}
        </div> : <div className="mod-empty-state"><PackageOpen size={20} aria-hidden="true" /><p>{en ? "No eligible Mods in this directory." : "此目录没有适用于当前操作的 Mod。"}</p></div>}
        <div className="mod-action-footer"><span role="status">{en ? `${batch.selected.length} selected` : `已选择 ${batch.selected.length} 项`}</span>
          <Button variant="primary" disabled={batch.loading || !batch.inspection || !batch.selected.length || !batch.accountId} onClick={batch.preview}>{en ? "Review processing plan" : "预览处理计划"}<ArrowRight size={14} aria-hidden="true" /></Button>
        </div>
      </section>
    </> : <section className="mod-processing-section mod-batch-plan">
      <div className="mod-section-toolbar"><div className="mod-section-heading"><h3>{en ? "Processing plan" : "处理计划"}</h3><p>{en ? "Check output names and results before starting." : "确认输出名称与检查结果后开始执行。"}</p></div>
        <span className="mod-batch-summary" role="status"><CheckCircle2 size={15} aria-hidden="true" />{en ? `${completed} of ${batch.rows.length} complete` : `${batch.rows.length} 项，已成功 ${completed} 项`}</span>
      </div>
      <div className="mod-batch-table-wrap"><table className="mod-batch-table"><thead><tr><th>{en ? "Source / original" : "来源／原始 Mod"}</th><th>{en ? "Target" : "目标"}</th><th>{en ? "Operation" : "操作"}</th><th>{en ? "Result" : "结果"}</th></tr></thead>
        <tbody>{batch.rows.map(row => <tr key={row.source} data-state={row.state}><td>{row.sourceModName ?? (en ? "Original game" : "原版游戏")}</td><td><strong>{row.name}</strong></td><td>{labels[row.mode]}<p className="micro-meta">{featureLabels.filter(([key]) => row.features[key]).map(([, label]) => label).join(en ? ", " : "、")}</p>{row.features.includeRoomTools && <p className="micro-meta">{row.features.roomToolsDefaultHell
            ? (en ? "Create game: default to Hell" : "创建房间：默认地狱")
            : (en ? "Create game: choose difficulty manually" : "创建房间：手动选择难度")}</p>}</td><td><span className="mod-batch-state" data-state={row.state}>{states[row.state]}</span>{row.message && <p>{row.message}</p>}
          {row.state === "running" && batch.progress && <><p>{batch.progress.message}</p><ProgressBar value={batch.progress.percent} label={row.name} /></>}</td></tr>)}</tbody></table></div>
      <div className="mod-action-footer">
        {batch.busy ? <><span role="status">{en ? "Processing in order" : "正在依次处理"}</span><Button variant="secondary" disabled={batch.stopping} onClick={batch.stop}>{batch.stopping ? (en ? "Stopping after current item…" : "当前项完成后停止…") : (en ? "Stop after current item" : "当前项完成后停止")}</Button></> : <>
          <Button variant="ghost" onClick={batch.edit}><ArrowLeft size={13} aria-hidden="true" />{en ? "Edit selection" : "返回修改"}</Button>
          <div className="mod-page-header-actions">{batch.rows.some(row => row.state === "failed" || row.state === "skipped") && <Button variant="secondary" onClick={() => void batch.run(true)}>{en ? "Retry failed / unstarted" : "重试失败及未开始项"}</Button>}
            {pending > 0 && <Button variant="primary" onClick={() => void batch.run()}><Layers3 size={14} aria-hidden="true" />{en ? `Process ${pending} available items` : `开始处理 ${pending} 个可执行项`}</Button>}</div>
        </>}
      </div>
    </section>}
  </div>;
}
