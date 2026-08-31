import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  AlertTriangle,
  CheckCircle2,
  Keyboard,
  MousePointerClick,
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
  RoomRotationPoint,
  RoomRotationUiProfile,
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

interface Props {
  config: GlobalConfig;
  accounts: AccountMeta[];
  updateConfig: (updater: (config: GlobalConfig) => void) => void;
}

type StrategyKey = "standard" | "direct_lobby";
type BackgroundClickStrategy = RoomRotationConfig["background_click_strategy"];
type ClickTestVariant = BackgroundClickStrategy | "cursor_guard_8" | "cursor_guard_16" | "cursor_guard_32";
type BackgroundTextStrategy = RoomRotationConfig["background_text_strategy"];

const DEFAULT_UI_PROFILE: RoomRotationUiProfile = {
  save_and_exit: { x: 500, y: 350 },
  character_select_lobby: { x: 583, y: 898 },
  create_tab: { x: 730, y: 20 },
  join_tab: { x: 820, y: 20 },
  game_name_field: { x: 696, y: 136 },
  password_field: { x: 696, y: 205 },
  submit_button: { x: 766, y: 625 },
  create_game_name_field: { x: 696, y: 136 },
  create_password_field: { x: 696, y: 205 },
  create_submit_button: { x: 766, y: 625 },
  join_game_name_field: { x: 696, y: 136 },
  join_password_field: { x: 696, y: 205 },
  join_submit_button: { x: 766, y: 625 },
  dialog_confirm: { x: 500, y: 560 },
};

const DEFAULT_STANDARD_FLOW: RoomRotationFlowStrategy = {
  click_lobby_after_exit: false,
  escape_to_exit_ms: 0,
  exit_load_ms: 0,
  lobby_load_ms: 0,
  step_delay_ms: 120,
  character_delay_ms: 10,
  ui_profile: DEFAULT_UI_PROFILE,
};

const DEFAULT_DIRECT_FLOW: RoomRotationFlowStrategy = {
  click_lobby_after_exit: false,
  escape_to_exit_ms: 0,
  exit_load_ms: 0,
  lobby_load_ms: 0,
  step_delay_ms: 80,
  character_delay_ms: 10,
  ui_profile: DEFAULT_UI_PROFILE,
};

const DEFAULT_CONFIG: RoomRotationConfig = {
  enabled: false,
  primary_account_id: "",
  follower_account_ids: [],
  shortcut: "Ctrl+Alt+R",
  join_shortcut: "Ctrl+Alt+J",
  name_prefix: "run-",
  password: "",
  next_sequence: 1,
  sequence_width: 3,
  input_mode: "cursor_guard",
  background_click_strategy: "post_top",
  background_text_strategy: "post_keys_paced",
  cursor_lease_ms: 16,
  frontend_timeout_ms: 12_000,
  create_timeout_ms: 10_000,
  ui_delay_ms: 550,
  follower_exit_delay_ms: 2_200,
  duplicate_retries: 3,
  ui_profile: DEFAULT_UI_PROFILE,
  strategy_version: 5,
  standard_flow: DEFAULT_STANDARD_FLOW,
  direct_lobby_flow: DEFAULT_DIRECT_FLOW,
  account_flow_bindings: {},
};

const STRATEGY_META: Record<StrategyKey, { label: string; description: string }> = {
  standard: {
    label: "局内工具 · 稳健",
    description: "直接点击 Mod 顶部工具栏，使用更宽松的表单间隔",
  },
  direct_lobby: {
    label: "局内工具 · 极速",
    description: "相同局内路径，缩短按钮、输入框和提交之间的间隔",
  },
};

const POINT_CONTROLS: Array<{
  key: keyof RoomRotationUiProfile;
  label: string;
  action: string;
  dangerous?: boolean;
}> = [
  { key: "create_tab", label: "局内创建按钮", action: "create_tab" },
  { key: "create_game_name_field", label: "创建 · 房间名", action: "create_game_name_field" },
  { key: "create_password_field", label: "创建 · 密码", action: "create_password_field" },
  { key: "create_submit_button", label: "创建 · 确认", action: "create_submit", dangerous: true },
  { key: "join_tab", label: "局内加入按钮", action: "join_tab" },
  { key: "join_game_name_field", label: "加入 · 房间名", action: "join_game_name_field" },
  { key: "join_password_field", label: "加入 · 密码", action: "join_password_field" },
  { key: "join_submit_button", label: "加入 · 确认", action: "join_submit", dangerous: true },
  { key: "dialog_confirm", label: "重名弹窗确认", action: "confirm", dangerous: true },
];

const CURSOR_GUARD_VARIANTS: Array<{
  key: ClickTestVariant;
  shortLabel: string;
  description: string;
}> = [
  { key: "cursor_guard_8", shortLabel: "E 极速 8ms", description: "最短光标占用，优先测试" },
  { key: "cursor_guard_16", shortLabel: "F 平衡 16ms", description: "约一帧光标占用" },
  { key: "cursor_guard_32", shortLabel: "G 稳定 32ms", description: "更长按键保持，兼容性优先" },
];

const BACKGROUND_TEXT_VARIANTS: ReadonlyArray<{
  key: BackgroundTextStrategy;
  shortLabel: string;
  description: string;
}> = [
  { key: "post_keys_paced", shortLabel: "L 跨帧逐字", description: "每键保持 30ms，再按当前流程的字符间隔逐字输入" },
  { key: "post_ctrl_v", shortLabel: "H 异步 Ctrl+V", description: "投递 Ctrl+A、退格与 Ctrl+V 按键消息" },
  { key: "send_ctrl_v", shortLabel: "I 同步 Ctrl+V", description: "同步发送同一组 Ctrl+V 按键消息" },
  { key: "post_paste", shortLabel: "J 异步 WM_PASTE", description: "选中并清空后异步发送系统粘贴消息" },
  { key: "send_paste", shortLabel: "K 同步 WM_PASTE", description: "选中并清空后同步发送系统粘贴消息" },
];

const KEY_TESTS: ReadonlyArray<{ key: string; label: string; dangerous?: boolean }> = [
  { key: "escape", label: "Esc" },
  { key: "tab", label: "Tab" },
  { key: "shift_tab", label: "Shift+Tab" },
  { key: "left", label: "←" },
  { key: "up", label: "↑" },
  { key: "right", label: "→" },
  { key: "down", label: "↓" },
  { key: "space", label: "Space", dangerous: true },
  { key: "enter", label: "Enter", dangerous: true },
];

function normalizedFlow(
  value: RoomRotationFlowStrategy | undefined,
  fallback: RoomRotationFlowStrategy,
  legacyProfile: RoomRotationUiProfile,
): RoomRotationFlowStrategy {
  return {
    ...fallback,
    ...value,
    ui_profile: {
      ...DEFAULT_UI_PROFILE,
      ...legacyProfile,
      ...value?.ui_profile,
    },
  };
}

function normalizedConfig(value?: RoomRotationConfig): RoomRotationConfig {
  if (!value) return DEFAULT_CONFIG;
  const legacyProfile = { ...DEFAULT_UI_PROFILE, ...value.ui_profile };
  return {
    ...DEFAULT_CONFIG,
    ...value,
    ui_profile: legacyProfile,
    standard_flow: normalizedFlow(value.standard_flow, DEFAULT_STANDARD_FLOW, legacyProfile),
    direct_lobby_flow: normalizedFlow(value.direct_lobby_flow, DEFAULT_DIRECT_FLOW, legacyProfile),
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
  const [testAccountId, setTestAccountId] = useState("");
  const [testBusy, setTestBusy] = useState<string | null>(null);
  const [activeStrategy, setActiveStrategy] = useState<StrategyKey>("standard");

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
    if (testAccountId && initializedAccounts.some(account => account.id === testAccountId)) return;
    setTestAccountId(rotation.primary_account_id || initializedAccounts[0]?.id || "");
  }, [initializedAccounts, rotation.primary_account_id, testAccountId]);

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
    if (!/^[A-Za-z0-9_-]+$/.test(rotation.name_prefix)) problems.push("房间前缀含无效字符");
    if (!/^[A-Za-z0-9_-]*$/.test(rotation.password) || rotation.password.length > 15) {
      problems.push("密码格式无效");
    }
    if (preview.length > 15) problems.push("房间名预览超过 15 个字符");
    return problems;
  }, [preview, rotation]);

  const activeFlow = activeStrategy === "standard" ? rotation.standard_flow : rotation.direct_lobby_flow;

  const patchRotation = (patch: Partial<RoomRotationConfig>) => {
    updateConfig(current => {
      current.room_rotation = { ...normalizedConfig(current.room_rotation), ...patch };
    });
  };

  const patchFlow = (strategy: StrategyKey, patch: Partial<RoomRotationFlowStrategy>) => {
    updateConfig(current => {
      const next = normalizedConfig(current.room_rotation);
      const field = strategy === "standard" ? "standard_flow" : "direct_lobby_flow";
      current.room_rotation = {
        ...next,
        [field]: { ...next[field], ...patch },
      };
    });
  };

  const patchPoint = (key: keyof RoomRotationUiProfile, patch: Partial<RoomRotationPoint>) => {
    updateConfig(current => {
      const next = normalizedConfig(current.room_rotation);
      const field = activeStrategy === "standard" ? "standard_flow" : "direct_lobby_flow";
      const flow = next[field];
      current.room_rotation = {
        ...next,
        [field]: {
          ...flow,
          ui_profile: {
            ...flow.ui_profile,
            [key]: { ...flow.ui_profile[key], ...patch },
          },
        },
      };
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

  const runInputTest = async (
    action: string,
    clickVariant?: ClickTestVariant,
    pointOverride?: RoomRotationPoint,
    textVariant?: BackgroundTextStrategy,
  ) => {
    if (!testAccountId) {
      showToast("warning", "请先选择测试账号");
      return;
    }
    const busyKey = textVariant
      ? `${action}:${textVariant}`
      : clickVariant
        ? `${action}:${clickVariant}`
        : action;
    const testedLease = clickVariant?.startsWith("cursor_guard_")
      ? Number(clickVariant.slice("cursor_guard_".length))
      : null;
    if (testedLease != null && Number.isFinite(testedLease)) {
      patchRotation({ input_mode: "cursor_guard", cursor_lease_ms: testedLease });
    }
    if (textVariant) {
      patchRotation({ input_mode: "cursor_guard", background_text_strategy: textVariant });
    }
    setTestBusy(busyKey);
    try {
      const message = await invoke<string>("test_room_rotation_input", {
        request: {
          accountId: testAccountId,
          action,
          sample: action.includes("password_text") ? rotation.password || "pass123" : preview,
          clickVariant,
          textVariant,
          flowStrategy: activeStrategy,
          pointOverride,
        },
      });
      showToast("success", message);
    } catch (error) {
      showToast("error", `后台输入测试失败: ${error}`);
    } finally {
      setTestBusy(null);
    }
  };

  const runStage = async (command: "start_room_rotation" | "join_room_rotation_followers") => {
    try {
      setStatus(await invoke<RoomRotationStatus>(command));
    } catch (error) {
      showToast("error", `自动换房启动失败: ${error}`);
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
            <span className="rounded-full border border-warning/30 bg-warning/10 px-2 py-0.5 text-[10px] font-semibold text-warning">
              人工确认进房
            </span>
          </div>
          <p className="mt-1 text-2xs leading-relaxed text-text-muted">
            主号快捷键在当前房间直接打开“创建房间”、填写并回车；确认主号进房后，再按小号快捷键，让全部小号从各自当前房间直接加入。
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

      <div className="space-y-2">
        <div className="flex items-center justify-between gap-2">
          <span className="text-xs font-semibold text-text-secondary">参与账号</span>
          <span className="text-2xs text-text-muted">所有账号需使用 r6 及以上加工 Mod</span>
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
          <div className={`flex h-8 items-center rounded-lg border px-2.5 font-mono text-xs ${preview.length > 15 ? "border-error text-error" : "border-success/30 bg-success/10 text-success"}`}>
            {preview}{rotation.password ? ` / ${rotation.password}` : " / 无密码"}
          </div>
        </label>
      </div>

      <div className="grid grid-cols-1 gap-2 md:grid-cols-3">
        <label className="space-y-1">
          <span className="text-2xs text-text-muted">自动点击模式</span>
          <select
            value={rotation.input_mode}
            onChange={event => patchRotation({ input_mode: event.target.value as RoomRotationConfig["input_mode"] })}
            className="h-8 w-full rounded-lg border border-border-default bg-surface-hover px-2.5 text-xs text-text-primary"
          >
            <option value="cursor_guard">受保护光标租约（推荐）</option>
            <option value="focus">短暂聚焦回退</option>
          </select>
        </label>
        <label className="space-y-1">
          <span className="text-2xs text-text-muted">后台点击光标租约</span>
          <select
            value={rotation.cursor_lease_ms}
            disabled={rotation.input_mode !== "cursor_guard"}
            onChange={event => patchRotation({ cursor_lease_ms: Number(event.target.value) })}
            className="h-8 w-full rounded-lg border border-border-default bg-surface-hover px-2.5 text-xs text-text-primary disabled:opacity-40"
          >
            <option value={8}>E 极速 · 8ms</option>
            <option value={16}>F 平衡 · 16ms</option>
            <option value={32}>G 稳定 · 32ms</option>
          </select>
        </label>
        <div className="flex items-end gap-2">
          <Button
            size="md"
            variant="primary"
            disabled={!rotation.enabled || status?.running}
            onClick={() => void runStage("start_room_rotation")}
            className="flex-1"
          >
            <Play size={12} />主号建房
          </Button>
          <Button
            size="md"
            disabled={!rotation.enabled || status?.running}
            onClick={() => void runStage("join_room_rotation_followers")}
            className="flex-1"
          >
            <Users size={12} />小号跟进
          </Button>
        </div>
      </div>
      <p className="text-2xs leading-relaxed text-text-secondary">
        不再发送 Esc、点击“保存并退出”或等待大厅。受保护光标模式只短暂占用局内按钮和输入框坐标；密码仅在该账号当前进程的创建/加入表单首次使用或配置变化时重写。
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
              <p className="text-2xs text-text-secondary">{status?.message ?? "主号快捷键建房；确认进房后按小号快捷键"}</p>
            </div>
          </div>
          {status?.running && (
            <Button size="sm" variant="danger" onClick={() => void stop()}>
              <Square size={11} />停止
            </Button>
          )}
        </div>
      </div>

      <details className="rounded-xl border border-border-default bg-surface-card px-3 py-2.5">
        <summary className="cursor-pointer text-xs font-semibold text-text-secondary">高级：速度与坐标校准</summary>
        <div className="mt-3 space-y-3">
          <div className="flex flex-wrap gap-2">
            {(Object.entries(STRATEGY_META) as Array<[StrategyKey, typeof STRATEGY_META.standard]>).map(([key, meta]) => (
              <Button
                key={key}
                size="sm"
                variant={activeStrategy === key ? "primary" : "secondary"}
                onClick={() => setActiveStrategy(key)}
              >
                {meta.label}
              </Button>
            ))}
          </div>
          <div>
            <p className="text-xs font-semibold text-text-primary">{STRATEGY_META[activeStrategy].label}</p>
            <p className="mt-0.5 text-2xs text-text-muted">{STRATEGY_META[activeStrategy].description}</p>
          </div>
          <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
            {([
              ["step_delay_ms", "表单步骤", 0, 2],
            ] as const).map(([key, label, min, max]) => (
              <label key={key} className="space-y-1">
                <span className="text-2xs text-text-muted">{label}（秒）</span>
                <input
                  type="number"
                  min={min}
                  max={max}
                  step={0.01}
                  value={activeFlow[key] / 1000}
                  onChange={event => patchFlow(activeStrategy, { [key]: Math.round(Math.max(min, Math.min(max, Number(event.target.value) || 0)) * 1000) })}
                  className="h-8 w-full rounded-lg border border-border-default bg-surface-hover px-2.5 font-mono text-xs text-text-primary"
                />
              </label>
            ))}
            <label className="space-y-1">
              <span className="text-2xs text-text-muted">字符释放间隔（毫秒）</span>
              <input
                type="number"
                min={5}
                max={250}
                step={1}
                value={activeFlow.character_delay_ms}
                onChange={event => patchFlow(activeStrategy, {
                  character_delay_ms: Math.round(Math.max(5, Math.min(250, Number(event.target.value) || 10))),
                })}
                className="h-8 w-full rounded-lg border border-border-default bg-surface-hover px-2.5 font-mono text-xs text-text-primary"
              />
            </label>
          </div>
          <p className="text-2xs leading-relaxed text-text-secondary">
            当前顺序：点击局内创建/加入 → 填写房间名 → 当前表单首次使用或密码变化时填写密码 → Enter。每轮都会重新点击局内按钮；不会缓存按钮状态，也不会回主界面或大厅。
          </p>
        </div>
      </details>

      <details className="rounded-xl border border-border-default bg-surface-card px-3 py-2.5">
        <summary className="cursor-pointer text-xs font-semibold text-text-secondary">局内工具坐标与单步测试</summary>
        <div className="mt-3 space-y-3">
          <div className="flex flex-wrap items-end gap-2">
            <label className="min-w-[180px] flex-1 space-y-1">
              <span className="text-2xs text-text-muted">测试账号</span>
              <select
                value={testAccountId}
                onChange={event => setTestAccountId(event.target.value)}
                className="h-8 w-full rounded-lg border border-border-default bg-surface-hover px-2.5 text-xs text-text-primary"
              >
                {initializedAccounts.map(account => (
                  <option key={account.id} value={account.id}>{account.display_name || account.id}</option>
                ))}
              </select>
            </label>
            <div className="min-w-[180px] flex-1 space-y-1">
              <span className="text-2xs text-text-muted">正在编辑与测试</span>
              <div className="flex h-8 items-center rounded-lg border border-border-default bg-surface-hover px-2.5 text-xs text-text-primary">
                {STRATEGY_META[activeStrategy].label}
              </div>
            </div>
            <Button size="sm" loading={testBusy === "escape"} onClick={() => void runInputTest("escape")}>
              <Keyboard size={11} />发送 Esc
            </Button>
            <Button size="sm" loading={testBusy === "create_name"} onClick={() => void runInputTest("create_name")}>
              <Keyboard size={11} />局内创建并填房名
            </Button>
            <Button size="sm" loading={testBusy === "join_name"} onClick={() => void runInputTest("join_name")}>
              <Keyboard size={11} />局内加入并填房名
            </Button>
            <Button size="sm" loading={testBusy === "create_password_text"} onClick={() => void runInputTest("create_password_text")}>
              <Keyboard size={11} />局内创建并填密码
            </Button>
            <Button size="sm" loading={testBusy === "join_password_text"} onClick={() => void runInputTest("join_password_text")}>
              <Keyboard size={11} />局内加入并填密码
            </Button>
          </div>
          <p className="text-2xs leading-relaxed text-warning">
            四个填字测试都会点击 Mod 顶部的局内创建/加入按钮，填写后停留 1 秒，再次点击同一按钮关闭表单；不会按 Enter，也不会把测试内容记作“密码已更新”。默认 L 方案不使用剪贴板，每键保持 30ms 并按当前流程设置留出释放间隔；H–K 仅保留用于兼容性诊断。
          </p>
          <p className="text-2xs leading-relaxed text-text-secondary">
            坐标相对 D2R 客户区：左上角 X 0%、Y 0%，右下角 X 100%、Y 100%。窗口移动、缩放或位于不同显示器时无需换算屏幕坐标。
          </p>
          <div className="grid grid-cols-1 gap-1.5 rounded-lg border border-border-default/70 bg-surface-hover px-2.5 py-2 md:grid-cols-3">
            {CURSOR_GUARD_VARIANTS.map(variant => (
              <div key={variant.key} className="text-2xs text-text-secondary">
                <span className="font-semibold text-text-primary">{variant.shortLabel}</span>
                <span> · {variant.description}</span>
              </div>
            ))}
            <p className="text-2xs leading-relaxed text-text-muted md:col-span-3">
              E–G 不激活目标窗口；点击期间锁定系统光标到目标点，完成后恢复原位置。并行小号也会串行占用这段极短的光标租约，避免互相覆盖坐标。
            </p>
          </div>
          <div className="space-y-2 rounded-lg border border-border-default/70 px-2.5 py-2.5">
            <div>
              <p className="text-xs font-semibold text-text-secondary">后台填字方案 · 加入页实测</p>
              <p className="mt-0.5 text-2xs leading-relaxed text-text-muted">
                L 会跨帧逐字输入；H–K 是剪贴板兼容性方案。都会执行“加入页签 → 加入房间名坐标 → 填写当前预览”，不按 Enter。
              </p>
            </div>
            <div className="flex flex-wrap gap-1.5">
              {BACKGROUND_TEXT_VARIANTS.map(variant => (
                <Button
                  key={variant.key}
                  size="sm"
                  variant={rotation.background_text_strategy === variant.key ? "primary" : "secondary"}
                  loading={testBusy === `join_name:${variant.key}`}
                  onClick={() => void runInputTest("join_name", undefined, undefined, variant.key)}
                  title={variant.description}
                >
                  <Keyboard size={11} />{variant.shortLabel}
                </Button>
              ))}
            </div>
          </div>
          <div className="grid grid-cols-1 gap-2">
            {POINT_CONTROLS.map(({ key, label, action, dangerous }) => {
              const point = activeFlow.ui_profile[key];
              return (
                <div key={key} className="flex flex-wrap items-center gap-2 rounded-lg border border-border-default/70 px-2.5 py-2">
                  <span className="min-w-[92px] text-2xs font-semibold text-text-secondary">{label}</span>
                  {(["x", "y"] as const).map(axis => (
                    <label key={axis} className="flex items-center gap-1 text-2xs text-text-muted">
                      {axis.toUpperCase()}%
                      <input
                        type="number"
                        min={0}
                        max={100}
                        step={0.1}
                        value={point[axis] / 10}
                        aria-label={`${label} ${axis.toUpperCase()} 坐标百分比`}
                        onChange={event => patchPoint(key, {
                          [axis]: Math.round(Math.min(100, Math.max(0, Number(event.target.value) || 0)) * 10),
                        })}
                        className="h-7 w-16 rounded border border-border-default bg-surface-hover px-1.5 font-mono text-xs text-text-primary"
                      />
                    </label>
                  ))}
                  <div className="ml-auto flex flex-wrap gap-1.5">
                    {CURSOR_GUARD_VARIANTS.map(variant => (
                      <Button
                        key={variant.key}
                        size="sm"
                        variant={dangerous ? "danger" : "secondary"}
                        loading={testBusy === `${action}:${variant.key}`}
                        onClick={() => void runInputTest(action, variant.key, point)}
                        aria-label={`${variant.shortLabel}测试点击${label}`}
                        title={variant.description}
                        className="px-2"
                      >
                        <MousePointerClick size={11} />{variant.shortLabel}
                      </Button>
                    ))}
                  </div>
                </div>
              );
            })}
          </div>
          <div className="space-y-2 rounded-lg border border-border-default/70 px-2.5 py-2.5">
            <div>
              <p className="text-xs font-semibold text-text-secondary">后台键盘导航诊断</p>
              <p className="mt-0.5 text-2xs leading-relaxed text-text-muted">两组均不移动鼠标、不激活窗口。</p>
            </div>
            {(["post", "send"] as const).map(delivery => (
              <div key={delivery} className="flex flex-wrap items-center gap-1.5">
                <span className="w-[142px] text-2xs font-semibold text-text-secondary">
                  {delivery === "post" ? "异步 PostMessage" : "同步 SendMessageTimeout"}
                </span>
                {KEY_TESTS.map(key => {
                  const action = `key_${delivery}_${key.key}`;
                  return (
                    <Button
                      key={key.key}
                      size="sm"
                      variant={key.dangerous ? "danger" : "secondary"}
                      loading={testBusy === action}
                      onClick={() => void runInputTest(action)}
                      className="min-w-[48px] px-2"
                    >
                      <Keyboard size={11} />{key.label}
                    </Button>
                  );
                })}
              </div>
            ))}
          </div>
        </div>
      </details>
    </div>
  );
}
