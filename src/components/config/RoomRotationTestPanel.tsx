import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  AlertTriangle,
  CheckCircle2,
  Keyboard,
  Play,
  Route,
  Square,
  Users,
} from "lucide-react";
import { parseShortcutFromKeyEvent } from "../../hooks/useShortcutRecorder";
import type {
  AccountMeta,
  GlobalConfig,
  RoomRotationConfig,
  RoomRotationFlowStrategy,
} from "../../store/types";
import { Button } from "../ui/Button";
import { showToast } from "../ui/Toast";
import { Toggle } from "../ui/Toggle";

interface RoomRotationStatus {
  running: boolean;
  phase: string;
  message: string;
  room_name: string | null;
  attempt: number;
  primary_account_id: string | null;
  follower_account_ids: string[];
  started_at: string | null;
  last_error: string | null;
}

interface ChatF13BindingStatus {
  ready: boolean;
  totalFiles: number;
  installedFiles: number;
  eligibleFiles: number;
  conflictedFiles: number;
  backupFiles: number;
  d2rRunning: boolean;
  autoPatchEnabled: boolean;
  directories: string[];
  message: string;
}

interface Props {
  config: GlobalConfig;
  accounts: AccountMeta[];
  updateConfig: (updater: (config: GlobalConfig) => void) => void;
}

const DEFAULT_STANDARD_FLOW: RoomRotationFlowStrategy = {
  step_delay_ms: 80,
  character_delay_ms: 10,
};

const DEFAULT_DIRECT_FLOW: RoomRotationFlowStrategy = {
  step_delay_ms: 60,
  character_delay_ms: 10,
};

const DEFAULT_CONFIG: RoomRotationConfig = {
  enabled: false,
  chat_f13_auto_patch_enabled: false,
  primary_account_id: "",
  follower_account_ids: [],
  auto_followers_enabled: false,
  auto_followers_delay_secs: 5,
  shortcut: "Ctrl+Alt+R",
  join_shortcut: "Ctrl+Alt+J",
  name_prefix: "run-",
  password: "",
  next_sequence: 1,
  sequence_width: 3,
  background_text_strategy: "post_keys",
  strategy_version: 16,
  standard_flow: DEFAULT_STANDARD_FLOW,
  direct_lobby_flow: DEFAULT_DIRECT_FLOW,
  account_flow_bindings: {},
};

function normalizedFlow(
  value: RoomRotationFlowStrategy | undefined,
  fallback: RoomRotationFlowStrategy,
): RoomRotationFlowStrategy {
  return {
    step_delay_ms: value?.step_delay_ms ?? fallback.step_delay_ms,
    character_delay_ms: value?.character_delay_ms ?? fallback.character_delay_ms,
  };
}

function normalizedConfig(value?: RoomRotationConfig): RoomRotationConfig {
  if (!value) return DEFAULT_CONFIG;
  return {
    enabled: value.enabled ?? DEFAULT_CONFIG.enabled,
    chat_f13_auto_patch_enabled: value.chat_f13_auto_patch_enabled ?? DEFAULT_CONFIG.chat_f13_auto_patch_enabled,
    primary_account_id: value.primary_account_id ?? DEFAULT_CONFIG.primary_account_id,
    follower_account_ids: value.follower_account_ids ?? DEFAULT_CONFIG.follower_account_ids,
    auto_followers_enabled: value.auto_followers_enabled ?? DEFAULT_CONFIG.auto_followers_enabled,
    auto_followers_delay_secs: value.auto_followers_delay_secs ?? DEFAULT_CONFIG.auto_followers_delay_secs,
    shortcut: value.shortcut ?? DEFAULT_CONFIG.shortcut,
    join_shortcut: value.join_shortcut ?? DEFAULT_CONFIG.join_shortcut,
    name_prefix: value.name_prefix ?? DEFAULT_CONFIG.name_prefix,
    password: value.password ?? DEFAULT_CONFIG.password,
    next_sequence: value.next_sequence ?? DEFAULT_CONFIG.next_sequence,
    sequence_width: value.sequence_width ?? DEFAULT_CONFIG.sequence_width,
    background_text_strategy: value.background_text_strategy ?? DEFAULT_CONFIG.background_text_strategy,
    strategy_version: value.strategy_version ?? DEFAULT_CONFIG.strategy_version,
    standard_flow: normalizedFlow(value.standard_flow, DEFAULT_STANDARD_FLOW),
    direct_lobby_flow: normalizedFlow(value.direct_lobby_flow, DEFAULT_DIRECT_FLOW),
    account_flow_bindings: value.account_flow_bindings ?? {},
  };
}

function phaseLabel(phase: string): string {
  const labels: Record<string, string> = {
    idle: "待命",
    starting_primary: "主号启动",
    opening_primary_room_form: "打开局内建房",
    creating_primary: "主号建房",
    retrying_primary: "重名重试",
    waiting_auto_followers: "等待自动跟进",
    ready_for_followers: "等待小号快捷键",
    joining_followers: "小号并行加入",
    complete: "已完成",
    cancelled: "已停止",
    error: "异常停止",
  };
  return labels[phase] ?? phase;
}

export function RoomRotationTestPanel({ config, accounts, updateConfig }: Props) {
  const rotation = normalizedConfig(config.room_rotation);
  const initializedAccounts = accounts.filter(account => account.initialized);
  const [status, setStatus] = useState<RoomRotationStatus | null>(null);
  const [recordingShortcut, setRecordingShortcut] = useState<"primary" | "followers" | null>(null);
  const [chatBinding, setChatBinding] = useState<ChatF13BindingStatus | null>(null);
  const [chatBindingBusy, setChatBindingBusy] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void invoke<RoomRotationStatus>("get_room_rotation_status")
      .then(next => {
        if (!cancelled) setStatus(next);
      })
      .catch(() => {});
    let unlisten: (() => void) | undefined;
    void listen<RoomRotationStatus>("room-rotation-status", event => {
      if (!cancelled) setStatus(event.payload);
    }).then(dispose => {
      if (cancelled) dispose();
      else unlisten = dispose;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    void invoke<ChatF13BindingStatus>("get_room_rotation_chat_binding_status")
      .then(next => {
        if (!cancelled) setChatBinding(next);
      })
      .catch(error => {
        if (!cancelled) {
          setChatBinding({
            ready: false,
            totalFiles: 0,
            installedFiles: 0,
            eligibleFiles: 0,
            conflictedFiles: 0,
            backupFiles: 0,
            d2rRunning: false,
            autoPatchEnabled: false,
            directories: [],
            message: String(error),
          });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [config.cn_saved_games_path, config.global_saved_games_path]);

  const preview = useMemo(() => {
    const number = String(Math.max(0, rotation.next_sequence)).padStart(rotation.sequence_width, "0");
    return `${rotation.name_prefix}${number}`;
  }, [rotation.name_prefix, rotation.next_sequence, rotation.sequence_width]);

  const readinessProblems = useMemo(() => {
    const problems: string[] = [];
    if (!rotation.primary_account_id) problems.push("请选择主号");
    if (rotation.follower_account_ids.length === 0) problems.push("至少选择一个小号");
    if (!rotation.shortcut) problems.push("请录制主号建房快捷键");
    if (!rotation.join_shortcut) problems.push("请录制小号跟进快捷键");
    if (rotation.shortcut && rotation.shortcut.toLowerCase() === rotation.join_shortcut.toLowerCase()) {
      problems.push("两个快捷键不能相同");
    }
    if (!/^[\p{L}\p{N}_-]+$/u.test(rotation.name_prefix)) problems.push("房间前缀含无效字符");
    if (!/^[\p{L}\p{N}_-]*$/u.test(rotation.password) || Array.from(rotation.password).length > 15) {
      problems.push("密码格式无效");
    }
    if (Array.from(preview).length > 15) problems.push("房间名预览超过 15 个字符");
    return problems;
  }, [preview, rotation]);

  const patchRotation = (patch: Partial<RoomRotationConfig>) => {
    updateConfig(current => {
      current.room_rotation = { ...normalizedConfig(current.room_rotation), ...patch };
    });
  };

  const toggleFollower = (accountId: string, checked: boolean) => {
    const followers = checked
      ? [...rotation.follower_account_ids, accountId]
      : rotation.follower_account_ids.filter(id => id !== accountId);
    patchRotation({ follower_account_ids: [...new Set(followers)] });
  };

  const recordShortcut = (
    target: "primary" | "followers",
    event: React.KeyboardEvent<HTMLInputElement>,
  ) => {
    event.preventDefault();
    event.stopPropagation();
    const shortcut = parseShortcutFromKeyEvent(event);
    if (!shortcut) return;
    patchRotation(target === "primary" ? { shortcut } : { join_shortcut: shortcut });
    setRecordingShortcut(null);
    event.currentTarget.blur();
  };

  const runStage = async (command: "start_room_rotation" | "join_room_rotation_followers") => {
    try {
      setStatus(await invoke<RoomRotationStatus>(command));
    } catch (error) {
      showToast("error", `自动换房启动失败: ${error}`);
    }
  };

  const runChatBindingAction = async (
    command:
      | "get_room_rotation_chat_binding_status"
      | "install_room_rotation_chat_binding"
      | "restore_room_rotation_chat_binding",
  ) => {
    setChatBindingBusy(command);
    try {
      const next = await invoke<ChatF13BindingStatus>(command);
      setChatBinding(next);
      if (command !== "get_room_rotation_chat_binding_status") {
        updateConfig(draft => {
          if (draft.room_rotation) {
            draft.room_rotation.chat_f13_auto_patch_enabled = next.autoPatchEnabled;
          }
        });
        showToast("success", next.message);
      }
    } catch (error) {
      showToast("error", `F13 原生文本键操作失败: ${error}`);
    } finally {
      setChatBindingBusy(null);
    }
  };

  const stop = async () => {
    try {
      setStatus(await invoke<RoomRotationStatus>("cancel_room_rotation"));
    } catch (error) {
      showToast("error", `停止换房失败: ${error}`);
    }
  };

  return (
    <div className="spatial-panel settings-span-full space-y-3 p-3">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 max-w-[72ch]">
          <div className="flex items-center gap-2">
            <Route size={15} className="text-accent" />
            <h3 className="text-xs font-bold text-text-primary">局内双阶段换房</h3>
            <span className={`rounded-full border px-2 py-0.5 text-[10px] font-semibold ${rotation.auto_followers_enabled ? "border-accent/30 bg-accent/10 text-accent" : "border-warning/30 bg-warning/10 text-warning"}`}>
              {rotation.auto_followers_enabled ? `${rotation.auto_followers_delay_secs} 秒自动跟进` : "手动确认进房"}
            </span>
          </div>
          <p className="mt-1 text-2xs leading-relaxed text-text-muted">
            主号快捷键在当前房间用纯键盘打开“创建房间”、填写并确认；{rotation.auto_followers_enabled ? `等待 ${rotation.auto_followers_delay_secs} 秒后，全部小号会在后台自动加入。` : "确认主号进房后，再按小号快捷键让全部小号在后台加入。"}
          </p>
          <p className="mt-1 text-2xs leading-relaxed text-warning">
            密码只在首次使用、进程重启或配置变化时填写。若提示房间名已存在，再按一次主号快捷键会自动换下一个序号重试。
          </p>
        </div>
        <Toggle
          checked={rotation.enabled}
          disabled={!rotation.enabled && readinessProblems.length > 0}
          ariaLabel="启用局内双阶段换房"
          onChange={enabled => patchRotation({ enabled })}
        />
      </div>

      <div className={`rounded-xl border px-3 py-2.5 ${chatBinding?.ready ? "border-success/30 bg-success/10" : "border-warning/30 bg-warning/10"}`}>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex min-w-0 items-start gap-2">
            <Keyboard size={14} className={`mt-0.5 shrink-0 ${chatBinding?.ready ? "text-success" : "text-warning"}`} />
            <div className="min-w-0">
              <p className="text-xs font-semibold text-text-primary">原生 Chat 备用键 · F13</p>
              <p className="text-2xs leading-relaxed text-text-secondary">
                {chatBinding?.message ?? "正在检查 .key/.keyo 键位文件…"}
              </p>
              <p className="mt-0.5 text-2xs leading-relaxed text-text-muted">
                只占用原生 Chat 的空闲第二键位，保留 Enter；启用后会在新账号或新角色的键位文件生成时自动补齐。恢复时只还原该键位，不覆盖其他后来修改的快捷键。
              </p>
            </div>
          </div>
          <div className="flex flex-wrap gap-1.5">
            <Button
              size="sm"
              loading={chatBindingBusy === "get_room_rotation_chat_binding_status"}
              onClick={() => void runChatBindingAction("get_room_rotation_chat_binding_status")}
            >
              刷新
            </Button>
            <Button
              size="sm"
              variant="primary"
              disabled={Boolean((chatBinding?.ready && chatBinding?.autoPatchEnabled) || chatBinding?.d2rRunning || chatBinding?.conflictedFiles)}
              loading={chatBindingBusy === "install_room_rotation_chat_binding"}
              onClick={() => void runChatBindingAction("install_room_rotation_chat_binding")}
            >
              {chatBinding?.ready ? "启用自动补齐" : "安装 F13"}
            </Button>
            <Button
              size="sm"
              disabled={!chatBinding?.backupFiles || chatBinding?.d2rRunning}
              loading={chatBindingBusy === "restore_room_rotation_chat_binding"}
              onClick={() => void runChatBindingAction("restore_room_rotation_chat_binding")}
            >
              恢复原键位
            </Button>
          </div>
        </div>
      </div>

      {readinessProblems.length > 0 && (
        <div className="flex items-start gap-2 rounded-xl border border-warning/25 bg-warning/10 px-3 py-2.5">
          <AlertTriangle size={14} className="mt-0.5 shrink-0 text-warning" />
          <p className="text-2xs leading-relaxed text-text-secondary">
            尚不能启用：{readinessProblems.join("；")}
          </p>
        </div>
      )}

      <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
        <label className="space-y-1.5">
          <span className="text-xs font-semibold text-text-secondary">主操作账号</span>
          <select
            value={rotation.primary_account_id}
            onChange={event => {
              const primary = event.target.value;
              patchRotation({
                primary_account_id: primary,
                follower_account_ids: rotation.follower_account_ids.filter(id => id !== primary),
              });
            }}
            className="h-8 w-full rounded-lg border border-border-default bg-surface-hover px-2.5 text-xs text-text-primary"
          >
            <option value="">请选择</option>
            {initializedAccounts.map(account => (
              <option key={account.id} value={account.id}>{account.display_name || account.id}</option>
            ))}
          </select>
        </label>

        {([
          ["primary", "主号建房快捷键", rotation.shortcut],
          ["followers", "小号跟进快捷键", rotation.join_shortcut],
        ] as const).map(([target, label, value]) => (
          <label key={target} className="space-y-1.5">
            <span className="text-xs font-semibold text-text-secondary">{label}</span>
            <div className="flex gap-2">
              <input
                readOnly
                value={recordingShortcut === target ? "请按组合键…" : value || "未配置"}
                onFocus={() => setRecordingShortcut(target)}
                onClick={() => setRecordingShortcut(target)}
                onKeyDown={event => recordShortcut(target, event)}
                className={`h-8 min-w-0 flex-1 rounded-lg border px-3 text-center font-mono text-xs outline-none ${
                  recordingShortcut === target
                    ? "border-accent bg-accent/10 text-accent"
                    : "border-border-default bg-surface-hover text-text-primary"
                }`}
              />
              <Button
                size="sm"
                onClick={() => patchRotation(target === "primary" ? { shortcut: "" } : { join_shortcut: "" })}
              >
                清除
              </Button>
            </div>
          </label>
        ))}
      </div>

      <div className={`rounded-xl border px-3 py-2.5 ${rotation.auto_followers_enabled ? "border-accent/30 bg-accent/10" : "border-border-default bg-surface-hover"}`}>
        <div className="flex flex-wrap items-center gap-3">
          <label className="flex min-w-[150px] items-center gap-2 text-xs font-semibold text-text-primary">
            <input
              type="checkbox"
              checked={rotation.auto_followers_enabled}
              onChange={event => patchRotation({ auto_followers_enabled: event.target.checked })}
            />
            主号后自动跟进
          </label>
          <input
            type="range"
            min={2}
            max={60}
            step={1}
            disabled={!rotation.auto_followers_enabled}
            value={rotation.auto_followers_delay_secs}
            aria-label="主号建房后自动跟进延迟秒数"
            onChange={event => patchRotation({
              auto_followers_delay_secs: Math.min(60, Math.max(2, Number(event.target.value) || 5)),
            })}
            className="min-w-[180px] flex-1 accent-accent disabled:opacity-40"
          />
          <output className="flex h-8 min-w-[58px] items-center justify-center rounded-lg border border-border-default bg-surface-card px-2 font-mono text-xs text-text-primary">
            {rotation.auto_followers_delay_secs} 秒
          </output>
        </div>
        <p className="mt-1.5 text-2xs leading-relaxed text-text-muted">
          默认关闭、手动按小号快捷键。开启后从主号提交建房开始计时，延迟结束直接调用同一套后台小号加入流程，不抢焦点，也不模拟全局快捷键。
        </p>
      </div>

      <div className="space-y-2">
        <div className="flex items-center justify-between gap-2">
          <span className="text-xs font-semibold text-text-secondary">参与账号</span>
          <span className="text-2xs text-text-muted">所有账号需使用含 r22 房间工具组的 Mod</span>
        </div>
        {rotation.primary_account_id && (
          <div className="flex flex-wrap items-center gap-2 rounded-lg border border-accent/25 bg-accent/10 px-2.5 py-2">
            <span className="min-w-[72px] text-2xs font-semibold text-accent">主号</span>
            <span className="min-w-[140px] flex-1 text-xs text-text-primary">
              {initializedAccounts.find(account => account.id === rotation.primary_account_id)?.display_name || rotation.primary_account_id}
            </span>
            <span className="text-2xs text-text-muted">局内创建</span>
          </div>
        )}
        <div className="grid grid-cols-1 gap-2 md:grid-cols-2">
          {initializedAccounts
            .filter(account => account.id !== rotation.primary_account_id)
            .map(account => {
              const checked = rotation.follower_account_ids.includes(account.id);
              return (
                <div
                  key={account.id}
                  className={`flex items-center gap-2 rounded-lg border px-2.5 py-2 ${
                    checked ? "border-border-strong bg-surface-hover" : "border-border-default/70"
                  }`}
                >
                  <input
                    type="checkbox"
                    checked={checked}
                    aria-label={`选择小号 ${account.display_name || account.id}`}
                    onChange={event => toggleFollower(account.id, event.target.checked)}
                  />
                  <span className="min-w-0 flex-1 truncate text-xs text-text-primary">{account.display_name || account.id}</span>
                   <span className="text-2xs text-text-muted">{checked ? "局内加入" : "未参与"}</span>
                </div>
              );
            })}
        </div>
      </div>

      <div className="grid grid-cols-2 gap-2 md:grid-cols-6">
        <label className="space-y-1">
          <span className="text-2xs text-text-muted">房间前缀</span>
          <input
            value={rotation.name_prefix}
            maxLength={14}
            onChange={event => patchRotation({ name_prefix: event.target.value })}
            className="h-8 w-full rounded-lg border border-border-default bg-surface-hover px-2.5 font-mono text-xs text-text-primary"
          />
        </label>
        <label className="space-y-1">
          <span className="text-2xs text-text-muted">下一个序号</span>
          <input
            type="number"
            min={0}
            value={rotation.next_sequence}
            onChange={event => patchRotation({ next_sequence: Math.max(0, Number(event.target.value) || 0) })}
            className="h-8 w-full rounded-lg border border-border-default bg-surface-hover px-2.5 font-mono text-xs text-text-primary"
          />
        </label>
        <label className="space-y-1">
          <span className="text-2xs text-text-muted">序号位数</span>
          <input
            type="number"
            min={1}
            max={6}
            value={rotation.sequence_width}
            onChange={event => patchRotation({ sequence_width: Math.min(6, Math.max(1, Number(event.target.value) || 1)) })}
            className="h-8 w-full rounded-lg border border-border-default bg-surface-hover px-2.5 font-mono text-xs text-text-primary"
          />
        </label>
        <label className="space-y-1">
          <span className="text-2xs text-text-muted">固定密码</span>
          <input
            value={rotation.password}
            maxLength={15}
            placeholder="可留空"
            onChange={event => patchRotation({ password: event.target.value })}
            className="h-8 w-full rounded-lg border border-border-default bg-surface-hover px-2.5 font-mono text-xs text-text-primary"
          />
        </label>
        <label className="space-y-1 md:col-span-2">
          <span className="text-2xs text-text-muted">本轮房间</span>
          <div className={`flex h-8 items-center rounded-lg border px-2.5 font-mono text-xs ${Array.from(preview).length > 15 ? "border-error text-error" : "border-success/30 bg-success/10 text-success"}`}>
            {preview}{rotation.password ? ` / ${rotation.password}` : " / 无密码"}
          </div>
        </label>
      </div>

      <div className="flex gap-2">
          <Button
            size="md"
            variant="primary"
            disabled={!rotation.enabled || !chatBinding?.ready || status?.running}
            onClick={() => void runStage("start_room_rotation")}
            className="flex-1"
          >
            <Play size={12} />主号建房
          </Button>
          <Button
            size="md"
            disabled={!rotation.enabled || !chatBinding?.ready || status?.running}
            onClick={() => void runStage("join_room_rotation_followers")}
            className="flex-1"
          >
            <Users size={12} />小号跟进
          </Button>
      </div>
      <p className="text-2xs leading-relaxed text-text-secondary">
        正式流程不激活 D2R、不移动鼠标，也不使用剪贴板：Esc 先落到无操作的安全焦点，连续两次左/右选中隐藏入口，确认键只会在入口选中成功后打开原生表单；随后 F13 进入屏蔽技能快捷键的文本态，Tab 切到密码并提交。即使方向消息漏掉，确认键也不会误触暂停菜单按钮。自动房名和密码支持英文、数字、短横线与下划线。
      </p>

      <div className={`rounded-xl border px-3 py-2.5 ${status?.running ? "border-accent/25 bg-accent/10" : status?.last_error ? "border-error/25 bg-error/10" : "border-border-default bg-surface-hover"}`}>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex min-w-0 items-center gap-2">
            {status?.last_error
              ? <AlertTriangle size={14} className="shrink-0 text-error" />
              : <CheckCircle2 size={14} className={`shrink-0 ${status?.running ? "text-accent" : "text-success"}`} />}
            <div className="min-w-0">
              <p className="text-xs font-semibold text-text-primary">
                {phaseLabel(status?.phase ?? "idle")}{status?.room_name ? ` · ${status.room_name}` : ""}
              </p>
              <p className="text-2xs text-text-secondary">{status?.message ?? (rotation.auto_followers_enabled ? "主号快捷键建房；延迟结束后自动跟进小号" : "主号快捷键建房；确认进房后按小号快捷键")}</p>
            </div>
          </div>
          {status?.running && (
            <Button size="sm" variant="danger" onClick={() => void stop()}>
              <Square size={11} />停止
            </Button>
          )}
        </div>
      </div>

    </div>
  );
}
