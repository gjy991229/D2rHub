import { Copy, Package } from "lucide-react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { Button } from "../../../components/ui/Button";
import { showToast } from "../../../components/ui/Toast";
import type { GlobalConfig, AudioModSetupState, ModCapsulePool } from "../../../store/types";
import { matchesRecognitionSource, type ExternalAudioTarget } from "../../../utils/recognitionSource";
import type { ExternalAudioInstance, RuneAudioStatus } from "../audioModuleModel";

interface Props {
  config: GlobalConfig;
  state: AudioModSetupState | null;
  status: RuneAudioStatus | null;
  pool: ModCapsulePool | null;
  busy: boolean;
  instances: ExternalAudioInstance[];
  instancesError: string | null;
  onChange: (target: ExternalAudioTarget) => Promise<void>;
  onSelectInstance: (instance: ExternalAudioInstance) => Promise<void>;
  onPrepare: () => void;
}

export function ExternalRecognitionTarget({ config, state, status, pool, busy, instances, instancesError, onChange, onSelectInstance, onPrepare }: Props) {
  const target = config.rune_audio_external_target;
  if (!target) return null;
  const en = config.app_language === "en-US";
  const editions = (["CN", "Global"] as const).filter(edition => (edition === "CN" ? config.cn_game_path : config.global_game_path)?.trim());
  const mods = pool?.capsules.filter(mod => mod.edition === target.edition && mod.ready && (mod.feature_groups.includes("audio_telemetry") || (!!config.hostile_auto_restart_enabled && mod.feature_groups.includes("hostile_auto_restart")))) ?? [];
  const argumentsText = state?.launch_arguments || (target.mod_name ? `-mod ${target.mod_name} -txt` : "");
  const selected = instances.find(instance => instance.selected);
  const identityValue = (instance: ExternalAudioInstance) => `${instance.pid}:${instance.started_at}`;
  const connected = config.rune_audio_enabled && status?.running === true && matchesRecognitionSource(config, status) && selected?.pid === status.target_pid;
  const recognized = connected && (status?.decoded_packets ?? 0) > 0;
  const selectionRequired = !selected && instances.some(instance => instance.ready);
  const waiting = !status?.last_error || status.last_error.startsWith("等待游戏运行");
  const message = !state?.ready ? (en ? "Prepare or select a recognition Mod" : "准备或选择识别 Mod")
    : recognized ? (en ? "Recognizing game audio" : "正在识别游戏声音")
      : connected ? (en ? "Connected · waiting for an audio marker" : "已连接，等待识别声纹")
        : selectionRequired ? (en ? "Select the game process to listen to" : "请选择要监听的游戏进程")
          : selected ? (en ? `Selected PID ${selected.pid}${config.rune_audio_enabled ? " · connecting" : " · ready to enable"}` : `已指定 PID ${selected.pid}${config.rune_audio_enabled ? "，正在连接" : "，开启后开始识别"}`)
            : config.rune_audio_enabled ? (waiting ? (en ? "Enabled · waiting for the game" : "已启用，等待游戏运行") : status?.last_error)
              : (en ? "Ready · launch a game and select its process" : "准备完成，启动游戏后指定进程");
  return <div className="recognition-target-section">
    <div className="recognition-readiness" role="status" aria-live="polite" id="rune-audio-readiness" data-state={recognized ? "running" : state?.ready ? "ready" : "attention"}>
      <p className="text-xs font-semibold text-text-primary">{message}</p>
      <p className="mt-1 text-2xs text-text-secondary">{en ? "Select a process even when only one game is running. Select again after restarting it." : "即使只运行一个游戏，也需要指定进程；游戏退出或重启后需重新选择。"}</p>
    </div>
    {editions.length > 1 && <label className="block text-xs text-text-secondary">
      {en ? "Game installation" : "游戏版本"}
      <select className="settings-input mt-1" value={target.edition} disabled={busy} onChange={event => void onChange({ edition: event.target.value as ExternalAudioTarget["edition"], mod_name: "" })}>
        {editions.map(edition => <option key={edition} value={edition}>{edition === "CN" ? (en ? "China" : "国服") : (en ? "Global" : "国际服")}</option>)}
      </select>
    </label>}
    <div className="recognition-capsule-selector">
      <select className="settings-input recognition-capsule-select" aria-label={en ? "Recognition Mod" : "识别 Mod"} value={target.mod_name} disabled={busy}
        onChange={event => void onChange({ ...target, mod_name: event.target.value })}>
        <option value="">{en ? "Choose a recognition Mod" : "选择已加工声纹的 Mod"}</option>
        {target.mod_name && !mods.some(mod => mod.name === target.mod_name) && <option value={target.mod_name}>{target.mod_name} · {en ? "requires inspection" : "待检查"}</option>}
        {mods.map(mod => <option key={mod.id} value={mod.name}>{mod.name}</option>)}
      </select>
      <Button size="sm" variant="secondary" disabled={busy} onClick={onPrepare}><Package size={12} />{state?.ready ? (en ? "Manage Mod" : "管理 Mod") : (en ? "Prepare Mod" : "准备 Mod")}</Button>
    </div>
    {argumentsText && <div className="recognition-launch-arguments">
      <span>{en ? "Game launch arguments" : "游戏启动参数"}：<code>{argumentsText}</code></span>
      <Button size="sm" variant="ghost" onClick={() => void writeText(argumentsText).then(() => showToast("success", en ? "Launch arguments copied" : "启动参数已复制"))
        .catch(error => showToast("error", `${en ? "Copy failed" : "复制失败"}：${error}`))}><Copy size={12} />{en ? "Copy" : "复制"}</Button>
    </div>}
    {argumentsText && <p className="text-2xs text-text-muted">{en ? "Set these arguments in Battle.net or your shortcut, then restart the game once." : "首次使用请将参数配置到战网或游戏快捷方式，并重启游戏。"}</p>}
    <label className="block text-xs text-text-secondary">
      {en ? "Game process" : "游戏进程"}
      <select className="settings-input mt-1" aria-label={en ? "Game process" : "游戏进程"} disabled={busy || !instances.length} value={selected ? identityValue(selected) : ""}
        onChange={event => { const instance = instances.find(instance => identityValue(instance) === event.target.value); if (instance?.ready) void onSelectInstance(instance); }}>
        <option value="" disabled>{instances.length ? (en ? "Select the game to listen to" : "指定要监听的游戏") : (en ? "No running game found" : "未检测到运行中的游戏")}</option>
        {instances.map(instance => <option key={identityValue(instance)} value={identityValue(instance)} disabled={!instance.ready} title={instance.message || undefined}>
          {`${instance.window_title || "D2R"} · PID ${instance.pid} · ${instance.mod_name ?? (en ? "No Mod" : "无 Mod")}${instance.ready ? "" : (en ? " · restart required" : " · 需检查参数")}`}
        </option>)}
      </select>
    </label>
    {instancesError && <p className="text-2xs text-warning" role="status">{instancesError}</p>}
    {!connected && instances.length === 1 && !instances[0].ready && <p className="text-2xs text-warning" role="status">{instances[0].message}</p>}
  </div>;
}
