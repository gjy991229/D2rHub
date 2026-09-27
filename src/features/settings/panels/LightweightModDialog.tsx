import { ProgressBar } from "../../../components/ui/ProgressBar";
import { useEffect, useRef, useState } from "react";
import { CheckCircle2, PackagePlus } from "lucide-react";
import { Modal } from "../../../components/ui/Modal";
import { Button } from "../../../components/ui/Button";
import { invokeCommand } from "../../../platform/tauri";
import type { AccountMeta, ModCapsule } from "../../../store/types";
import type { ModCapsuleController } from "../../modCapsules/useModCapsulePool";
import { LIGHTWEIGHT_PROFILES, lightweightNameError, nextLightweightName, type LightweightContext, type LightweightProfile, type LightweightResult } from "../../modCapsules/lightweightModel";
import { taskGateway } from "../../tasks/gateway";
import { subscribeBeforeReadingTasks } from "../../tasks/taskSync";
import type { TaskSnapshot } from "../../tasks/types";
import "./lightweightMod.css";

interface Props {
  open: boolean; onClose: () => void; edition: string; isEnglish: boolean;
  catalog: ModCapsuleController; accounts: AccountMeta[];
  onProcess: (capsule: ModCapsule) => Promise<void> | void;
  onGenerated: (name: string) => void;
}
export function LightweightModDialog({ open, onClose, edition, isEnglish: en, catalog, accounts, onProcess, onGenerated }: Props) {
  const [profile, setProfile] = useState<LightweightProfile>("main");
  const [name, setName] = useState("LiteHub");
  const [context, setContext] = useState<LightweightContext | null>(null);
  const [contextLoading, setContextLoading] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [task, setTask] = useState<TaskSnapshot | null>(null);
  const [result, setResult] = useState<LightweightResult | null>(null);
  const [accountId, setAccountId] = useState("");
  const [applying, setApplying] = useState(false);
  const [applied, setApplied] = useState(false);
  const trackedTask = useRef<number | null>(null);
  const handledTask = useRef<number | null>(null);
  const refresh = catalog.refresh;
  const running = submitting || task?.state === "running";
  const existing = catalog.pool?.capsules.find((c) => c.edition === edition && c.origin === "scanned" && c.name.toLowerCase() === name.toLowerCase());
  const sameExisting = existing?.ready && existing.lightweight_profile === profile;
  const completed = result ? catalog.pool?.capsules.find((c) => c.edition === result.edition && c.name === result.mod_name && c.ready) : undefined;
  const availableAccounts = accounts.filter((a) => catalog.pool?.accounts.some((entry) => entry.account_id === a.id && entry.edition === (result?.edition ?? edition)));
  const nameError = lightweightNameError(name, en);
  const alternateName = nextLightweightName(name, (catalog.pool?.capsules ?? []).filter((c) => c.edition === edition).map((c) => c.name));
  const select = (id: LightweightProfile) => {
    setProfile(id); setError(null); setApplied(false);
    if (LIGHTWEIGHT_PROFILES.some((p) => p.name === name) || !name) setName(LIGHTWEIGHT_PROFILES.find((p) => p.id === id)!.name);
  };
  useEffect(() => {
    setResult(null); setAccountId(""); setApplied(false); setError(null);
  }, [edition]);
  useEffect(() => {
    if (!open) return;
    let live = true;
    setContext(null); setContextLoading(true); setError(null);
    void invokeCommand<LightweightContext>("get_lightweight_mod_context", { edition }).then((value) => { if (live) setContext(value); })
      .catch((e) => { if (live) setError(String(e)); }).finally(() => { if (live) setContextLoading(false); });
    void refresh();
    return () => { live = false; };
  }, [open, edition, refresh]);
  useEffect(() => {
    if (!open) return;
    let live = true; let stop: (() => void) | undefined;
    void subscribeBeforeReadingTasks(taskGateway, (snapshot) => {
      if (!live) return;
      const entries = [...snapshot.values()].filter((t) => t.kind === "lightweight-mod-generate");
      const active = entries.find((t) => t.state === "running");
      if (active) trackedTask.current = active.task_id;
      const current = active ?? entries.find((t) => t.task_id === trackedTask.current);
      if (!current) return;
      setTask(current);
      if (current.state !== "running" && handledTask.current !== current.task_id) {
        handledTask.current = current.task_id;
        void refresh();
        if (current.state === "failed" || current.state === "cancelled") setError(current.message);
      }
    }).then((s) => { if (!live) s(); else stop = s; }).catch((e) => { if (live) setError(String(e)); });
    return () => { live = false; stop?.(); };
  }, [open, refresh]);
  const generate = async () => {
    if (running || nameError || existing || !context?.available) return;
    setSubmitting(true); setResult(null); setError(null); setApplied(false); setTask(null); trackedTask.current = null;
    try {
      const value = await invokeCommand<LightweightResult>("generate_lightweight_mod", { edition, profile, modName: name });
      setResult(value);
      if (!await refresh()) setError(en ? "Generated successfully. Refresh the Mod list to continue." : "已生成成功，请刷新 Mod 列表后继续操作。");
      onGenerated(value.mod_name);
    } catch (e) { setError(String(e)); } finally { setSubmitting(false); }
  };
  const useExisting = () => {
    if (!existing || !sameExisting) return;
    setResult({ edition, profile, mod_name: existing.name, mod_directory: "", launch_arguments: existing.launch_arguments, task_id: 0 });
    setError(null); setAccountId(""); setApplied(false); onGenerated(existing.name);
  };
  const cancel = async () => {
    if (!task || task.state !== "running") return;
    try { setTask(await taskGateway.cancel(task.task_id)); } catch (e) { setError(String(e)); }
  };
  return <Modal open={open} onClose={onClose} width="max-w-xl" title={en ? "Generate lightweight Mod" : "生成轻量 Mod"}
    footer={<>
      <Button variant="ghost" onClick={onClose}>{running ? (en ? "Run in background" : "后台运行") : (en ? "Close" : "关闭")}</Button>
      {running ? <Button disabled={!task || task.cancel_requested} onClick={() => void cancel()}>{task?.cancel_requested ? (en ? "Cancelling…" : "正在取消…") : (en ? "Cancel generation" : "取消生成")}</Button>
        : !result && (sameExisting ? <Button variant="primary" onClick={useExisting}>{en ? "Use existing Mod" : "使用现有成品"}</Button>
          : <Button variant="primary" disabled={contextLoading || !context?.available || !!nameError || !!existing || catalog.loading} onClick={() => void generate()}><PackagePlus size={14} />{error ? (en ? "Retry generation" : "重新生成") : (en ? "Start generation" : "开始生成")}</Button>)}
    </>}>
    <div className="lightweight-dialog">
      <p className="lightweight-intro">{en ? "Generate from local game resources, then assign the Mod to your accounts." : "使用本机游戏资源生成，完成后可分配给账号。"}</p>
      <div className="lightweight-environment"><strong>{edition === "CN" ? (en ? "China" : "国服") : (en ? "Global" : "国际服")}</strong>
        <span>{contextLoading ? (en ? "Checking game directory…" : "正在检查游戏目录…") : context?.game_directory || context?.reason}</span></div>
      {!result && <>
        <fieldset className="lightweight-profiles" disabled={running}><legend>{en ? "Choose a profile" : "选择方案"}</legend>
          {LIGHTWEIGHT_PROFILES.map((p) => <label key={p.id} data-selected={profile === p.id}>
            <input type="radio" name="lightweight-profile" value={p.id} checked={profile === p.id} onChange={() => select(p.id)} />
            <span><strong>{p.name}<small>{en ? p.en : p.label}</small></strong><span>{en ? p.enDetail : p.detail}</span></span>
          </label>)}
        </fieldset>
        {profile === "min" && <p className="lightweight-note">{en ? "Cursors and maps are displayed smaller in this profile." : "此方案的光标和地图显示较小。"}</p>}
        <label className="lightweight-name">{en ? "Mod name" : "Mod 名称"}
          <input className="settings-input" value={name} maxLength={64} disabled={running} autoCapitalize="off" autoCorrect="off" spellCheck={false} onChange={(e) => { setName(e.target.value); setError(null); }} aria-invalid={!!nameError} />
        </label>
        {nameError && <p className="lightweight-error" role="alert">{nameError}</p>}
        {existing && !running && <div className="lightweight-existing">
          <span>{sameExisting ? (en ? "This profile already exists." : "此方案已生成，可直接使用。") : (en ? "This name is occupied. Choose another name." : "此名称已被其他内容占用，请修改名称。")}</span>
          <Button size="sm" variant="ghost" onClick={() => setName(alternateName)}>{en ? `Save as ${alternateName}` : `另存为 ${alternateName}`}</Button>
        </div>}
      </>}
      {running && <div className="lightweight-progress" role="status" aria-live="polite"><span>{task?.message || (en ? "Starting generation…" : "正在开始生成…")}</span><ProgressBar value={task?.progress} label={en ? "Mod generation progress" : "Mod 生成进度"} /><small>{en ? "You can close this dialog and follow progress in Background tasks." : "可以关闭弹窗，进度会保留在后台任务中。"}</small></div>}
      {error && <p className="lightweight-error" role="alert">{error}</p>}
      {result && !running && <section className="lightweight-success">
        <h3><CheckCircle2 size={18} />{result.mod_name} {en ? "is ready" : "已就绪"}</h3>
        <p>{en ? "Choose an account to use it on the next game launch." : "选择账号后应用，在该账号下次启动游戏时生效。"}</p>
        <label>{en ? "Assign to account" : "分配给账号"}<select className="settings-input" value={accountId} disabled={applying} onChange={(e) => { setAccountId(e.target.value); setApplied(false); }}>
          <option value="">{availableAccounts.length ? (en ? "Choose an account" : "请选择账号") : (en ? "No initialized account for this edition" : "暂无该版本的已初始化账号")}</option>
          {availableAccounts.map((a) => <option key={a.id} value={a.id}>{a.display_name || a.id}</option>)}
        </select></label>
        <div className="lightweight-success-actions"><Button variant="primary" disabled={!accountId || !completed || applying || applied} loading={applying} onClick={() => {
          if (!completed) return; setApplying(true); setError(null);
          void catalog.assign(accountId, completed.id).then((pool) => { if (!pool) setError(en ? "Account assignment failed. Please retry." : "账号应用失败，请重试。"); else setApplied(true); }).catch((e) => setError(String(e))).finally(() => setApplying(false));
        }}>{applied ? (en ? "Applied · next launch" : "已应用 · 下次启动生效") : (en ? "Apply to account" : "应用到账号")}</Button>
          <Button disabled={!completed || applying} onClick={() => { if (completed) { onClose(); void onProcess(completed); } }}>{en ? "Continue processing" : "继续加工"}</Button>
          <Button variant="ghost" disabled={applying} onClick={() => { setResult(null); setTask(null); setError(null); setAccountId(""); }}>{en ? "Generate another" : "再生成一个"}</Button></div>
      </section>}
    </div>
  </Modal>;
}
