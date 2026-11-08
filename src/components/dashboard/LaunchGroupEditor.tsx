import { switchPrimary } from "../../features/roomAutomation/roles";
import { useState } from "react";
import { createPortal } from "react-dom";
import { ArrowDown, ArrowUp, Copy, Trash2 } from "lucide-react";
import type { LaunchGroupController } from "../../hooks/useLaunchGroupController";
import { useGlobalConfig } from "../../store/globalConfig";
import { useAccounts } from "../../store/accounts";
import type { ModCapsulePool, SchemeEnvironment } from "../../store/types";
import { DisplaySection, GraphicsSection, AudioSection, GameplaySection, AutomapSection } from "../../features/settings/gameSettings/fields";
import { Modal } from "../ui/Modal";
import { invokeCommand } from "../../platform/tauri";
import { getLayoutMonitors } from "../../features/windowLayouts/gateway";
import { presetSlots } from "../../features/windowLayouts/model";
import type { RoomAutomationConfigSnapshot } from "../../features/roomAutomation/types";
import { showToast } from "../ui/Toast";
import "./launchSchemeEditor.css";
import { SchemeLayoutPreview } from "./SchemeLayoutPreview";

export function LaunchGroupEditor({ launchGroups: controller, modCapsulePool, onOpenSettings, hidden = false }: {
  launchGroups: LaunchGroupController; modCapsulePool?: ModCapsulePool | null;
  onOpenSettings?: (tab: string) => void;
  hidden?: boolean;
}) {
  const config = useGlobalConfig(state => state.config);
  const accounts = useAccounts(state => state.accounts);
  const [tab, setTab] = useState("accounts");
  const [layoutBusy, setLayoutBusy] = useState(false);
  const draft = controller.draft;
  if (!draft || !config || controller.pendingDelete || hidden) return null;
  const en = config.app_language === "en-US";
  const env = draft.environment, room = env.room, settings = env.settings;
  const selected = accounts.filter(account => draft.members.some(member => member.account_id === account.id));
  const name = (id: string) => accounts.find(account => account.id === id)?.display_name || id;
  const edit = controller.updateEnvironment;
  const setting = <K extends keyof SchemeEnvironment["settings"]>(key: K, value: SchemeEnvironment["settings"][K]) => edit(env => { env.settings[key] = value; });
  const errors: string[] = [];
  {
    if (room.primary_account_id && !selected.some(account => account.id === room.primary_account_id)) errors.push("跟房：请选择方案内的主号");
    if (room.follower_account_ids.some(id => !selected.some(account => account.id === id) || id === room.primary_account_id)) errors.push("跟房：请选择有效的小号");
  }
  if (settings.rune_audio_target_account && !selected.some(account => account.id === settings.rune_audio_target_account)) errors.push("识别：请选择方案内的账号");
  const ready = draft.name.trim() && draft.members.length && draft.members.every(member => member.snapshot && member.resolution && member.fps != null) && !errors.length;
  const createLayout = async () => {
    setLayoutBusy(true);
    try {
      const monitors = await getLayoutMonitors();
      if (!monitors.length) throw new Error("未检测到显示器");
      edit(env => { env.layout = { id: crypto.randomUUID(), name: "方案布局", monitors, windows: presetSlots(monitors, Math.max(1, draft.members.length), "1080p") }; });
    } catch (error) { showToast("error", String(error)); }
    finally { setLayoutBusy(false); }
  };
  const importCurrent = async () => {
    setLayoutBusy(true);
    try {
      let currentRoom = room;
      if (config.installed_optional_modules?.includes("room-automation")) currentRoom = (await invokeCommand<RoomAutomationConfigSnapshot>("room_automation_get_config")).config;
      edit(env => {
        for (const key of Object.keys(env.settings) as (keyof SchemeEnvironment["settings"])[]) {
          const value = config[key];
          if (value !== undefined) Object.assign(env.settings, { [key]: structuredClone(value) });
        }
        if (config.rune_audio_external_target) env.settings.rune_audio_target_account = "";
        env.room = { primary_account_id: currentRoom.primary_account_id, follower_account_ids: [...currentRoom.follower_account_ids] };
        const ids = new Set(draft.members.map(member => member.account_id));
        if (!ids.has(env.room.primary_account_id)) env.room.primary_account_id = "";
        env.room.follower_account_ids = env.room.follower_account_ids.filter(id => ids.has(id));
        if (!ids.has(env.settings.rune_audio_target_account)) env.settings.rune_audio_target_account = "";
        if (!ids.has(env.settings.keep_battle_net_account_id ?? "")) env.settings.keep_battle_net_account_id = null;
        env.layout = structuredClone(config.window_layout_enabled ? config.window_layouts?.find(layout => layout.id === config.active_window_layout_id) ?? null : null);
      });
      showToast("info", "已填入草稿，检查后保存方案；当前运行不会改变");
    } catch (error) { showToast("error", `读取当前设置失败：${error}`); }
    finally { setLayoutBusy(false); }
  };
  const options = <><option value="">不指定账号</option>{selected.map(account => <option key={account.id} value={account.id}>{name(account.id)}</option>)}</>;
  return createPortal(<Modal open title={draft.id ? `编辑方案：${draft.name}` : "新建启动方案"} width="max-w-4xl"
    onClose={controller.cancelDraft} dismissible={!controller.busy && !layoutBusy} footer={<>
      {draft.id && <button className="control-btn" disabled={controller.busy} onClick={controller.duplicate}><Copy size={14} />复制为新方案</button>}
      <button className="control-btn" disabled={controller.busy || layoutBusy} onClick={controller.cancelDraft}>取消</button>
      <button className="primary-cta" disabled={!ready || controller.busy || layoutBusy} onClick={() => void controller.saveDraft()}>{controller.busy ? "正在保存…" : "保存方案"}</button>
    </>}>
    <div className="scheme-form">
      <label>方案名称<input autoFocus maxLength={32} value={draft.name} disabled={controller.busy} onChange={event => controller.renameDraft(event.target.value)} /></label>
      <p className="scheme-note" data-i18n-skip>{en ? "Save to use these values on future scheme launches. Running games and account defaults stay unchanged." : "保存后在下次按此方案启动时使用，当前游戏与账号默认配置保持不变。"}</p>
      <nav className="scheme-tabs" aria-label="方案设置">{[["accounts", "账号与启动"], ["room", "跟房"], ["recognition", "识别"], ["layout", "窗口布局"]].map(([id, label]) => <button type="button" key={id} aria-pressed={tab === id} onClick={() => setTab(id)}>{label}</button>)}</nav>
      <fieldset disabled={controller.busy || layoutBusy} className="scheme-body">
        {tab === "accounts" && <>
          <div className="scheme-account-picker" aria-label="选择方案账号">{accounts.map(account => <label key={account.id}><input type="checkbox" disabled={!account.initialized} checked={draft.members.some(member => member.account_id === account.id)} onChange={() => controller.toggleAccount(account.id)} />{name(account.id)}{!account.initialized && "（未初始化）"}</label>)}</div>
          {!draft.members.length && <p className="scheme-note">先选择账号，再配置跟房、识别和布局。</p>}
          {draft.members.map((member, index) => {
            const account = accounts.find(account => account.id === member.account_id), snapshot = member.snapshot;
            const update = (patch: Partial<typeof member>) => controller.updateMember(member.account_id, patch);
            const layoutSlot = env.layout?.windows[index];
            const edition = modCapsulePool?.accounts.find(account => account.account_id === member.account_id)?.edition;
            const mods = modCapsulePool?.capsules.filter(capsule => capsule.edition === edition && capsule.ready) ?? [];
            return <section className="scheme-account" key={member.account_id}>
              <div className="scheme-row"><strong>{index + 1}. {name(member.account_id)}</strong><span className="configuration-origin-tag" data-i18n-skip>{en ? "Scheme values" : "方案值"}</span><span className="scheme-spacer" />
                <button className="icon-btn" aria-label={`上移 ${name(member.account_id)}`} disabled={index === 0} onClick={() => controller.moveMember(member.account_id, -1)}><ArrowUp size={14}/></button>
                <button className="icon-btn" aria-label={`下移 ${name(member.account_id)}`} disabled={index === draft.members.length - 1} onClick={() => controller.moveMember(member.account_id, 1)}><ArrowDown size={14}/></button>
                <button className="icon-btn" aria-label={`移除 ${name(member.account_id)}`} onClick={() => controller.toggleAccount(member.account_id)}><Trash2 size={14}/></button>
              </div>
              <div className="scheme-fields">
                <label>Mod<select value={member.mod_args ?? ""} onChange={event => update({ mod_args: event.target.value })}><option value="">不使用 Mod</option>{member.mod_args && !mods.some(mod => mod.launch_arguments === member.mod_args) && <option value={member.mod_args}>{member.mod_args}</option>}{mods.map(mod => <option key={mod.id} value={mod.launch_arguments}>{mod.name}</option>)}</select></label>
                <label>分辨率{layoutSlot ? <span className="scheme-note" data-i18n-skip>{en ? `Scheme layout · Slot ${index + 1}` : `方案布局 · 位置 ${index + 1}`}<br/>{layoutSlot.width} × {layoutSlot.height}</span> : <input aria-label={`${name(member.account_id)} 分辨率`} value={member.resolution ?? ""} placeholder="1920x1080" disabled={!snapshot} onChange={event => update({ resolution: event.target.value, graphics_configured: true })}/>}</label>
                <label>FPS（0 为不限）<input type="number" min={0} max={500} disabled={!snapshot} value={member.fps ?? 30} onChange={event => update({ fps: Number(event.target.value), graphics_configured: true })}/></label>
              </div>
              {!snapshot ? <p className="scheme-note">正在读取账号默认设置…</p> : <details><summary>位置、语言与启动参数</summary><div className="scheme-fields">
                <label>位置<select disabled={!!layoutSlot} value={snapshot.position ? member.position_preset_id ?? "snapshot" : ""} onChange={event => { const position = account?.position_presets?.find(position => position.id === event.target.value); update({ position_preset_id: position?.id ?? null, snapshot: { ...snapshot, position: position ? [position.x, position.y] : null } }); }}><option value="">不指定位置</option>{snapshot.position && <option value={member.position_preset_id ?? "snapshot"}>已保存位置（{snapshot.position.join(", ")}）</option>}{account?.position_presets?.filter(position => position.id !== member.position_preset_id).map(position => <option key={position.id} value={position.id}>{position.name}</option>)}</select></label>
                {!layoutSlot && snapshot.position && <>{["X", "Y"].map((axis, coordinate) => <label key={axis}>{axis}<input type="number" value={snapshot.position![coordinate]} onChange={event => { const position: [number, number] = [...snapshot.position!]; position[coordinate] = Number(event.target.value); update({ snapshot: { ...snapshot, position } }); }}/></label>)}</>}
                <label>区服<select disabled={account?.auth_mode !== "token"} value={snapshot.region ?? ""} onChange={event => update({ snapshot: { ...snapshot, region: event.target.value } })}>{(snapshot.region === "CN" ? ["CN"] : ["KR", "NA", "EU"]).map(region => <option key={region} value={region}>{({ CN: "国服", KR: "亚洲", NA: "美洲", EU: "欧洲" } as Record<string, string>)[region]}</option>)}</select></label>
                <label>文字语言<input value={snapshot.language ?? ""} placeholder="跟随游戏默认" onChange={event => update({ snapshot: { ...snapshot, language: event.target.value || null } })}/></label>
                <label>语音语言<input value={snapshot.voicelanguage ?? ""} placeholder="跟随游戏默认" onChange={event => update({ snapshot: { ...snapshot, voicelanguage: event.target.value || null } })}/></label>
                <label className="scheme-wide">启动参数<input value={member.mod_args ?? ""} onChange={event => update({ mod_args: event.target.value })}/></label>
              </div></details>}
              {snapshot && <><label className="scheme-toggle"><input type="checkbox" checked={Object.keys(snapshot.game_settings).length > 0} onChange={event => {
                if (!event.target.checked) { update({ snapshot: { ...snapshot, game_settings: {} } }); return; }
                setLayoutBusy(true);
                void invokeCommand<Record<string, unknown>>("get_account_settings", { accountId: member.account_id, defaultsOnly: true })
                  .then(settings => { if (!Object.keys(settings).length) throw new Error("账号游戏设置为空"); update({ snapshot: { ...snapshot, game_settings: settings } }); })
                  .catch(error => showToast("error", `读取账号设置失败：${error}`)).finally(() => setLayoutBusy(false));
              }}/>覆盖账号游戏设置</label>{Object.keys(snapshot.game_settings).length > 0 && <details><summary>独立游戏设置</summary>{[...(!env.layout ? [DisplaySection] : []), GraphicsSection, AudioSection, GameplaySection, AutomapSection].map((Section, sectionIndex) => <Section key={sectionIndex} settings={{ ...snapshot.game_settings, "Screen Resolution (Windowed)": member.resolution, "Framerate Cap": member.fps }} update={(key, value) => {
                if (key === "Screen Resolution (Windowed)") update({ resolution: String(value) });
                else if (key === "Framerate Cap" || key === "Framerate Target") update({ fps: Number(value) });
                else update({ snapshot: { ...snapshot, game_settings: { ...snapshot.game_settings, [key]: value } } });
              }}/>)}</details>}</>}
            </section>;
          })}
          <div className="scheme-fields"><label>保留战网<select value={settings.keep_battle_net_account_id ?? ""} onChange={event => setting("keep_battle_net_account_id", event.target.value || null)}><option value="">不保留</option>{selected.filter(account => account.auth_mode === "bnet").map(account => <option key={account.id} value={account.id}>{name(account.id)}</option>)}</select></label></div>
        </>}
        {tab === "room" && <>
          <div className="scheme-fields"><label>主号<select value={room.primary_account_id} onChange={event => edit(env => { env.room = switchPrimary(env.room, event.target.value); })}>{options}</select></label></div>
          <p>跟随账号（按勾选顺序派发）</p><div className="scheme-account-picker">{selected.filter(account => account.id !== room.primary_account_id).map(account => <label key={account.id}><input type="checkbox" checked={room.follower_account_ids.includes(account.id)} onChange={event => edit(env => { env.room.follower_account_ids = event.target.checked ? [...env.room.follower_account_ids, account.id] : env.room.follower_account_ids.filter(id => id !== account.id); })}/>{name(account.id)}{room.follower_account_ids.includes(account.id) && ` · ${room.follower_account_ids.indexOf(account.id) + 1}`}</label>)}</div>
          <p className="scheme-note">不指定任何账号时，本方案不使用跟房。参与账号随方案保存；快捷键、延时、跟随方式、协作模式及房间规则使用公共设置；序号持续递增。{onOpenSettings && <button type="button" className="control-btn" onClick={() => onOpenSettings("room-automation")}>前往公共跟房设置</button>}</p>
          <p className="scheme-note">公共模块关闭时不运行。启动方案只准备跟房账号，不会自动建房或进房。参与账号需要带房间工具的 Mod。</p>
        </>}
        {tab === "recognition" && <>
          <div className="scheme-fields"><label>识别账号<select value={settings.rune_audio_target_account} onChange={event => setting("rune_audio_target_account", event.target.value)}>{options}</select></label>{room.primary_account_id && <button type="button" className="control-btn" onClick={() => setting("rune_audio_target_account", room.primary_account_id)}>使用跟房主号</button>}</div>
          <p className="scheme-note">不指定账号时，本方案不使用识别。监听账号随方案保存；识别开关、记录与过滤规则、识别参数及悬浮窗均使用公共设置。{onOpenSettings && <button type="button" className="control-btn" onClick={() => onOpenSettings("automation")}>前往公共识别设置</button>}</p>
          <p className="scheme-note">改变跟房主号不会自动改变已选的识别账号；公共模块关闭时不运行。</p>
        </>}
        {tab === "layout" && <>
          <div className="scheme-fields"><label>布局模板<select value="" onChange={event => { const layout = config.window_layouts?.find(layout => layout.id === event.target.value); if (layout) edit(env => { env.layout = structuredClone(layout); }); }}><option value="">选择模板，复制到方案…</option>{config.window_layouts?.map(layout => <option key={layout.id} value={layout.id}>{layout.name}</option>)}</select></label><button className="control-btn" disabled={!draft.members.length} onClick={() => void createLayout()}>生成 {draft.members.length || 1} 窗口布局</button><button className="control-btn" onClick={() => edit(env => { env.layout = null; })}>不使用统一布局</button></div>
          {env.layout ? <><SchemeLayoutPreview layout={env.layout} names={draft.members.map(member => name(member.account_id))}/><p className="scheme-note">布局只在启动或点击应用时使用。位置不足时，多余账号沿用自身位置；启动后可自由调整。</p><label>布局名称<input value={env.layout.name} onChange={event => edit(env => { env.layout!.name = event.target.value; })}/></label>
          {draft.members.map((member, index) => { const slot = env.layout!.windows[index]; return slot && <section className="scheme-account" key={member.account_id}><strong>位置 {index + 1} · {name(member.account_id)}</strong><div className="scheme-fields"><label>显示器<select value={slot.monitor_id} onChange={event => edit(env => { env.layout!.windows[index].monitor_id = event.target.value; })}>{env.layout!.monitors.map(monitor => <option key={monitor.id} value={monitor.id}>{monitor.name}</option>)}</select></label>{([["x", "X"], ["y", "Y"], ["width", "分辨率宽"], ["height", "分辨率高"]] as const).map(([key, label]) => <label key={key}>{label}<input type="number" value={slot[key]} onChange={event => edit(env => { env.layout!.windows[index][key] = Number(event.target.value); })}/></label>)}</div></section>; })}</> : <p className="scheme-note">使用方案内各账号自己的位置与分辨率。</p>}
        </>}
      </fieldset>
      {controller.loadError && <p role="alert" className="scheme-error">{controller.loadError}。可移除后重新添加该账号以重试。</p>}
      {!!errors.length && <div role="alert" className="scheme-error">{errors.map(error => <p key={error}>{error}</p>)}</div>}
      <div className="scheme-row"><button className="control-btn" disabled={controller.busy || layoutBusy} onClick={() => void importCurrent()}>从当前账号选择和布局填入</button><span className="scheme-spacer"/>{draft.id && <button className="control-btn" disabled={controller.busy || layoutBusy} onClick={controller.requestDraftDelete}>删除方案</button>}</div>
    </div>
  </Modal>, document.body);
}
