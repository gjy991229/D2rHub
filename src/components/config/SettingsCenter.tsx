import { useState, useEffect, useRef } from "react";
import {
  Settings,
  Folder,
  User,
  Palette,
  ScanEye,
  Cat,
  ShieldAlert,
  FolderOpen,
  RotateCw,
  X,
  Play,
  Download,
  Upload,
  CheckCircle2,
  AlertTriangle,
  Package,
  ChevronDown,
  LocateFixed,
  MonitorUp,
} from "lucide-react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { useGlobalConfig } from "../../store/globalConfig";
import { useAccounts } from "../../store/accounts";
import { useTheme } from "../../store/theme";
import { Modal } from "../ui/Modal";
import { Button } from "../ui/Button";
import { Toggle } from "../ui/Toggle";
import { Input } from "../ui/Input";
import { showToast } from "../ui/Toast";
import { parseShortcutFromKeyEvent, useShortcutRecorder } from "../../hooks/useShortcutRecorder";
import {
  DisplaySection,
  GraphicsSection,
  AudioSection,
  GameplaySection,
  AutomapSection,
  SettingsMap
} from "../../pages/SettingsEditor";
import type { AudioModSetupState, GlobalConfig } from "../../store/types";
import { validateTrackingTarget } from "../../utils/trackingTarget";
import { installationPathEditsAreInvalid } from "../../utils/installationPathChanges";
import {
  AUDIO_MOD_NAME_MAX_LENGTH,
  validateAudioModName,
} from "../../utils/audioModName";
import { sortAccountsByCardOrder } from "../../utils/accountOrder";
import {
  locateAuxiliaryWindow,
  recoverAuxiliaryWindows,
  setAuxiliaryWindowVisible,
  type AuxiliaryWindowLabel,
} from "../../utils/windowPlacement";
import { RoomRotationTestPanel } from "./RoomRotationTestPanel";

// Helper for quadratic opacity mapping
// Map slider value s (0..100) to stored percentage p (10..100)
function sliderToPercent(s: number): number {
  return Math.round(10 + 0.009 * s * s);
}

// Map stored percentage p (10..100) to slider value s (0..100)
function percentToSlider(p: number): number {
  if (p <= 10) return 0;
  return Math.round(100 * Math.sqrt((p - 10) / 90));
}

interface Props {
  open: boolean;
  onClose: () => void;
  onReconfigure: () => void;
  onInitializeAccount: () => void;
  initialTab?: string | null;
  initialAccountId?: string | null;
}

interface ExportAccountsSummary {
  path: string;
  account_count: number;
  plaintext_token_count: number;
}

interface ImportAccountsSummary {
  imported: { id: string; display_name: string; initialized: boolean }[];
  warnings: string[];
  reencrypted_token_count: number;
}

interface RuneAudioStatus {
  running: boolean;
  account_id: string | null;
  target_pid: number | null;
  last_error: string | null;
  captured_frames: number;
  audio_peak: number;
  decoded_packets: number;
  rune_events: number;
  item_events: number;
  scene_heartbeats: number;
  last_marker: string | null;
  last_confidence: number | null;
  last_detected_at: string | null;
  diagnostic_recording: boolean;
  diagnostic_recording_path: string | null;
}

interface AudioModPrepareProgress {
  account_id: string;
  phase: string;
  percent: number;
  message: string;
}

interface AudioModPrepareResult {
  account_id: string;
  mod_name: string;
  mod_directory: string;
  launch_arguments: string;
  source_mod_name: string | null;
}

function audioSetupDefaults(state: AudioModSetupState): {
  mode: "original" | "existing";
  source: string;
  name: string;
} {
  const sources = state.installed_mods.filter((mod) => mod.source_eligible);
  const recordedSource = sources.find((mod) => (
    !!state.source_mod_name && mod.name.toLowerCase() === state.source_mod_name.toLowerCase()
  ));
  const source = recordedSource
    ?? (state.update_required && state.build_mode === "augment" && sources.length === 1 ? sources[0] : undefined)
    ?? (!state.update_required ? sources[0] : undefined);
  const preserveExisting = state.update_required
    ? state.build_mode === "augment"
    : !!source;
  return {
    mode: preserveExisting ? "existing" : "original",
    source: source?.name ?? "",
    name: state.update_required ? state.current_mod_name ?? "" : "",
  };
}

const TRACKING_CATEGORIES = [
  { id: "runes", label: "符文", detail: "#1–#33" },
  { id: "gems", label: "宝石与骷髅", detail: "35 种等级/颜色" },
  { id: "charms", label: "护身符", detail: "小型/大型/超大型；不区分词缀" },
  { id: "jewels", label: "珠宝", detail: "基础珠宝；不区分品质或词缀" },
  { id: "keys", label: "钥匙", detail: "恐惧/憎恨/毁灭" },
  { id: "organs", label: "器官", detail: "角/眼/脑" },
  { id: "essences", label: "精华与徽章", detail: "四种精华及赦免徽章" },
] as const;

const DEFAULT_TRACKING_CATEGORIES = TRACKING_CATEGORIES.map(category => category.id);
const GEM_LEVELS = ["碎裂", "裂开", "普通", "无瑕疵", "完美"] as const;
const CHARM_FILTERS = [
  { code: "cm1", label: "小型护身符", detail: "Small Charm" },
  { code: "cm2", label: "大型护身符", detail: "Large Charm" },
  { code: "cm3", label: "超大型护身符", detail: "Grand Charm" },
] as const;
const AGGREGATE_ITEM_FILTERS = [
  { id: "jewels", label: "珠宝", detail: "全部基础珠宝，不区分品质或词缀" },
  { id: "keys", label: "钥匙", detail: "恐惧、憎恨、毁灭三把钥匙" },
  { id: "organs", label: "器官", detail: "角、眼、脑作为一整项" },
  { id: "essences", label: "精华与徽章", detail: "四种精华及赦免徽章" },
] as const;
type InstallationPathField =
  | "cn_battle_net_path"
  | "cn_game_path"
  | "cn_saved_games_path"
  | "global_game_path"
  | "global_saved_games_path";

interface InstallationProfileFieldsProps {
  edition: "CN" | "Global";
  config: GlobalConfig;
  settingsAvailable: boolean | null;
  detectedSavedGames: string | null;
  updateConfig: (updater: (config: GlobalConfig) => void) => void;
  pickFile: (field: keyof GlobalConfig, title: string, extensions?: string[]) => Promise<void>;
  pickFolder: (field: keyof GlobalConfig, title: string) => Promise<void>;
  applyDetectedPath: (field: keyof GlobalConfig, value: string | null) => void;
}

function InstallationProfileFields({
  edition,
  config,
  settingsAvailable,
  detectedSavedGames,
  updateConfig,
  pickFile,
  pickFolder,
  applyDetectedPath,
}: InstallationProfileFieldsProps) {
  const isCn = edition === "CN";
  const label = isCn ? "国服" : "国际服";
  const fields: Record<"game" | "savedGames", InstallationPathField> = isCn
    ? {
        game: "cn_game_path",
        savedGames: "cn_saved_games_path",
      }
    : {
        game: "global_game_path",
        savedGames: "global_saved_games_path",
      };
  const hasConfiguration = Object.values(fields).some((field) => config[field])
    || (isCn && Boolean(config.cn_battle_net_path));
  const profileComplete = Boolean(config[fields.game].trim());

  const clearProfile = () => {
    updateConfig((next) => {
      if (isCn) next.cn_battle_net_path = "";
      next[fields.game] = "";
      next[fields.savedGames] = "";
    });
  };

  return (
    <div className="space-y-2 border-t border-border-default/50 pt-3">
      <div className="flex items-center justify-between gap-3">
        <div>
          <p className="text-xs font-semibold text-text-secondary">{label}</p>
          <p className="text-2xs text-text-muted mt-0.5">
            {isCn
              ? "游戏目录可支撑核心启动；战网认证还需客户端路径，存档目录仅供画质覆盖。"
              : "亚/美/欧服共用，仅支持 Token 直启；存档目录仅供画质覆盖。"}
          </p>
        </div>
        {hasConfiguration && <Button size="sm" onClick={clearProfile}>清除此版本</Button>}
      </div>

      {hasConfiguration && !profileComplete && (
        <p className="text-xs text-text-muted leading-relaxed">
          当前版本尚未配置游戏安装目录；不会开放该版本的账号创建与启动。
        </p>
      )}

      {isCn && (
        <>
          <label className="text-xs text-text-muted block">国服战网客户端 (Battle.net.exe)</label>
          <div className="flex gap-2">
            <input
              type="text"
              value={config.cn_battle_net_path}
              readOnly
              className="flex-1 h-8 px-3 rounded-lg bg-surface-hover text-xs border border-border-default text-text-primary"
            />
            <Button size="sm" onClick={() => pickFile("cn_battle_net_path", "国服 Battle.net.exe", ["exe"])}>浏览</Button>
          </div>
        </>
      )}

      <label className="text-xs text-text-muted block">游戏安装目录</label>
      <div className="flex gap-2">
        <input
          type="text"
          value={config[fields.game]}
          readOnly
          className="flex-1 h-8 px-3 rounded-lg bg-surface-hover text-xs border border-border-default text-text-primary"
        />
        <Button size="sm" onClick={() => pickFolder(fields.game, `${label}游戏安装目录`)}>浏览</Button>
      </div>

      <label className="text-xs text-text-muted block">
        存档目录（可选） · Diablo II Resurrected{isCn ? " (CN)" : ""}
      </label>
      <div className="flex gap-2">
        <input
          type="text"
          value={config[fields.savedGames]}
          readOnly
          className="flex-1 h-8 px-3 rounded-lg bg-surface-hover text-xs border border-border-default text-text-primary"
        />
        <Button size="sm" onClick={() => pickFolder(fields.savedGames, `${label}存档目录`)}>浏览</Button>
        <Button size="sm" onClick={() => applyDetectedPath(fields.savedGames, detectedSavedGames)}>自动探测</Button>
      </div>

      {settingsAvailable === false && (
        <div
          className="flex items-start gap-2 px-3 py-2.5 rounded-lg"
          style={{ background: "var(--toast-warning-bg)", border: "1px solid var(--toast-warning-border)" }}
        >
          <ShieldAlert size={14} className="text-warning shrink-0 mt-0.5" />
          <p className="text-xs text-text-secondary leading-relaxed">
            {label}存档目录中未检测到 Settings.json，账号独立画质快照与覆盖暂不可用。
          </p>
        </div>
      )}
    </div>
  );
}

type TabType =
  | "paths"
  | "accounts"
  | "agent"
  | "appearance"
  | "automation"
  | "pet"
  | "shortcuts"
  | "advanced";

export function SettingsCenter({ open, onClose, onReconfigure, onInitializeAccount, initialTab, initialAccountId }: Props) {
  const { config, save, detectSavedGamesPath, detectGlobalSavedGamesPath, detectProgramDataAgentPath, detectAppDataRoamingBnetPath, detectBrowserPath } = useGlobalConfig();
  const { accounts, loadAccounts, renameAccount, updateAccountMods } = useAccounts();
  const { theme, setTheme } = useTheme();
  const initializedTrackingAccounts = accounts.filter((account) => account.initialized);
  const shortcutAccounts = sortAccountsByCardOrder(accounts);
  const trackingTarget = validateTrackingTarget(config?.rune_audio_target_account ?? "", accounts);
  const trackingTargetId = trackingTarget.valid ? trackingTarget.account.id : "";

  // Tab and search state
  const [activeTab, setActiveTab] = useState<TabType>("accounts");
  const [settingsJsonAvailable, setSettingsJsonAvailable] = useState<Record<"CN" | "Global", boolean | null>>({ CN: null, Global: null });
  const [audioStatus, setAudioStatus] = useState<RuneAudioStatus | null>(null);
  const [audioModState, setAudioModState] = useState<AudioModSetupState | null>(null);
  const [audioModStateLoading, setAudioModStateLoading] = useState(false);
  const [audioSetupOpen, setAudioSetupOpen] = useState(false);
  const [audioSetupMode, setAudioSetupMode] = useState<"original" | "existing">("original");
  const [audioSetupSource, setAudioSetupSource] = useState("");
  const [audioSetupName, setAudioSetupName] = useState("");
  const [audioPreparing, setAudioPreparing] = useState(false);
  const [audioPrepareProgress, setAudioPrepareProgress] = useState<AudioModPrepareProgress | null>(null);
  const [windowPlacementBusy, setWindowPlacementBusy] = useState<string | null>(null);
  const normalizedAudioSetupName = audioSetupName.trim();
  const isAudioModUpgrade = !!audioModState?.update_required;
  const installedAudioModNames = audioModState?.installed_mods.map((mod) => mod.name) ?? [];
  const audioSetupNameError = isAudioModUpgrade
    ? ""
    : validateAudioModName(audioSetupName, installedAudioModNames);
  const showAudioSetupNameError = !isAudioModUpgrade && audioSetupName.length > 0 && !!audioSetupNameError;
  const hasInitializedAudioAccount = initializedTrackingAccounts.length > 0;
  const hasAudioTarget = trackingTarget.valid;
  const hasReadyAudioMod = hasAudioTarget && !!audioModState?.ready;
  const isAudioEnableRequested = !!config?.rune_audio_enabled;
  const isAudioRecognitionActive = isAudioEnableRequested && hasReadyAudioMod;
  const audioPrepareBlockedReason = !isAudioModUpgrade && audioSetupNameError
    ? audioSetupNameError
    : audioSetupMode === "existing" && !audioSetupSource
      ? "请选择一个要保留功能的原始 Mod"
      : "";

  const locateWindow = async (label: AuxiliaryWindowLabel) => {
    const names = config?.app_language === "en-US"
      ? { overlay: "Terror Zone Broadcast", "stats-overlay": "Run Statistics", "bongo-cat": "Cat Overlay" }
      : { overlay: "邪恶区域播报窗口", "stats-overlay": "场景统计窗口", "bongo-cat": "猫咪悬浮窗" };
    const name = names[label];
    setWindowPlacementBusy(label);
    try {
      await locateAuxiliaryWindow(label);
      showToast(
        "success",
        config?.app_language === "en-US"
          ? `${name} was moved to this display`
          : `${name}已移到当前屏幕`,
      );
    } catch (error) {
      showToast(
        "error",
        config?.app_language === "en-US"
          ? `Failed to locate ${name}: ${error}`
          : `定位${name}失败: ${error}`,
      );
    } finally {
      setWindowPlacementBusy(null);
    }
  };

  const recoverAllWindows = async () => {
    setWindowPlacementBusy("all");
    try {
      const recovered = await recoverAuxiliaryWindows("main");
      if (recovered.length === 0) {
        showToast("info", "当前没有已启用的悬浮窗");
      } else {
        showToast(
          "success",
          config?.app_language === "en-US"
            ? `Moved ${recovered.length} overlay windows to this display`
            : `已将 ${recovered.length} 个悬浮窗移到当前屏幕`,
        );
      }
    } catch (error) {
      showToast(
        "error",
        config?.app_language === "en-US"
          ? `Failed to recover overlay windows: ${error}`
          : `找回悬浮窗失败: ${error}`,
      );
    } finally {
      setWindowPlacementBusy(null);
    }
  };

  // Config backup for rollback
  const [originalConfig, setOriginalConfig] = useState<GlobalConfig | null>(null);
  const autoSaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let active = true;
    const check = async (edition: "CN" | "Global", path: string | undefined) => {
      if (!open || !path) {
        if (active) setSettingsJsonAvailable(previous => ({ ...previous, [edition]: null }));
        return;
      }
      try {
        const exists = await invoke<boolean>("check_saved_games_settings", { path });
        if (active) setSettingsJsonAvailable(previous => ({ ...previous, [edition]: exists }));
      } catch {
        if (active) setSettingsJsonAvailable(previous => ({ ...previous, [edition]: false }));
      }
    };
    void check("CN", config?.cn_saved_games_path);
    void check("Global", config?.global_saved_games_path);
    return () => {
      active = false;
    };
  }, [open, config?.cn_saved_games_path, config?.global_saved_games_path]);

  useEffect(() => {
    if (!open || activeTab !== "automation") return;
    let cancelled = false;
    let timer: number | undefined;
    const poll = async () => {
      try {
        const next = await invoke<RuneAudioStatus>("get_rune_audio_status");
        if (!cancelled) setAudioStatus(next);
      } catch (error) {
        console.warn("读取音频遥测状态失败", error);
      }
      if (!cancelled) timer = window.setTimeout(poll, 1000);
    };
    void poll();
    return () => {
      cancelled = true;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [open, activeTab]);

  useEffect(() => {
    if (!open || activeTab !== "automation" || !trackingTargetId) {
      setAudioModState(null);
      return;
    }
    let cancelled = false;
    setAudioModStateLoading(true);
    void invoke<AudioModSetupState>("get_audio_mod_setup_state", { accountId: trackingTargetId })
      .then((next) => {
        if (cancelled) return;
        setAudioModState(next);
        const defaults = audioSetupDefaults(next);
        const availableSources = next.installed_mods.filter((mod) => mod.source_eligible);
        setAudioSetupSource((current) => (
          current && availableSources.some((mod) => mod.name === current)
            ? current
            : defaults.source
        ));
        setAudioSetupMode(defaults.mode);
        setAudioSetupName(defaults.name);
        if (!next.ready && config?.rune_audio_enabled) setAudioSetupOpen(true);
      })
      .catch((error) => {
        if (!cancelled) {
          setAudioModState(null);
          console.warn("读取识别 Mod 状态失败", error);
        }
      })
      .finally(() => {
        if (!cancelled) setAudioModStateLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, activeTab, trackingTargetId, config?.rune_audio_enabled]);

  useEffect(() => {
    if (!open || activeTab !== "automation") return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listen<AudioModPrepareProgress>("audio-mod-prepare-progress", (event) => {
      if (!cancelled) setAudioPrepareProgress(event.payload);
    }).then((stopListening) => {
      if (cancelled) stopListening();
      else unlisten = stopListening;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [open, activeTab]);

  // Game settings edit states (per account)
  const [selectedAccountId, setSelectedAccountId] = useState<string>("");
  const [gameSettings, setGameSettings] = useState<SettingsMap>({});
  const [gameSettingsLoading, setGameSettingsLoading] = useState(false);
  const [gameSettingsLoadError, setGameSettingsLoadError] = useState<string | null>(null);
  const [gameSettingsChanged, setGameSettingsChanged] = useState(false);
  const [gameSettingsSaving, setGameSettingsSaving] = useState(false);
  const [_fontScaleKey, setFontScaleKey] = useState(0);
  const [gameSettingsTab, setGameSettingsTab] = useState<"launch" | "game_display" | "game_graphics" | "game_audio" | "game_gameplay" | "game_automap">("launch");

  // Account card values draft states
  const [accountNicknameDraft, setAccountNicknameDraft] = useState("");
  const [accountModArgsDraft, setAccountModArgsDraft] = useState("");
  const [accountWinXDraft, setAccountWinXDraft] = useState<number | null>(null);
  const [accountWinYDraft, setAccountWinYDraft] = useState<number | null>(null);

  // Shortcut key recording state (shared hook)
  const { recordingPos, setRecordingPos } = useShortcutRecorder();

  // Local detected paths
  const [detectedPaths, setDetectedPaths] = useState<Record<string, string | null>>({});
  const [exportPickerOpen, setExportPickerOpen] = useState(false);
  const [exportAccountIds, setExportAccountIds] = useState<string[]>([]);
  const [exportPlaintextRiskAcknowledged, setExportPlaintextRiskAcknowledged] = useState(false);
  const [accountTransferBusy, setAccountTransferBusy] = useState<"export" | "import" | null>(null);

  // Auto initialize selected account and active tab
  // Backup config for rollback when modal opens
  useEffect(() => {
    if (open && config) {
      setOriginalConfig(JSON.parse(JSON.stringify(config)));
    }
  }, [open]);

  useEffect(() => {
    if (!open) {
      setExportPickerOpen(false);
      setExportPlaintextRiskAcknowledged(false);
    }
  }, [open]);

  // Auto initialize selected account and active tab
  useEffect(() => {
    if (open) {
      if (initialTab) {
        setActiveTab(initialTab as TabType);
      } else {
        setActiveTab("accounts");
      }

      // Pre-select account if provided
      const activeAccounts = accounts.filter(a => a.initialized);
      if (initialAccountId) {
        setSelectedAccountId(initialAccountId);
      } else if (activeAccounts.length > 0) {
        setSelectedAccountId(activeAccounts[0].id);
      } else if (accounts.length > 0) {
        setSelectedAccountId(accounts[0].id);
      }

      // Detect paths
      (async () => {
        const cnSavedGames = await detectSavedGamesPath();
        const globalSavedGames = await detectGlobalSavedGamesPath();
        const agent = await detectProgramDataAgentPath();
        const roaming = await detectAppDataRoamingBnetPath();
        const browser = await detectBrowserPath();
        setDetectedPaths({
          cnSavedGames,
          globalSavedGames,
          agent,
          roaming,
          browser: browser ? browser[0] : null,
        });
      })();
    }
  }, [open, initialTab, initialAccountId]);

  // Load account parameters draft when selected account changes
  useEffect(() => {
    if (selectedAccountId) {
      const acc = accounts.find(a => a.id === selectedAccountId);
      if (acc) {
        setAccountNicknameDraft(acc.display_name || acc.id);
        setAccountModArgsDraft(acc.mod_args || "");
        setAccountWinXDraft(acc.window_x !== undefined ? acc.window_x : null);
        setAccountWinYDraft(acc.window_y !== undefined ? acc.window_y : null);
        loadGameSettings(selectedAccountId);
      }
    }
  }, [selectedAccountId, accounts]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    const setupListener = async () => {
      const stopListening = await listen<{ accountId: string }>("account-settings-updated", (event) => {
        if (event.payload.accountId === selectedAccountId && !gameSettingsSaving) {
          loadGameSettings(selectedAccountId);
        }
      });
      if (cancelled) stopListening();
      else unlisten = stopListening;
    };
    if (selectedAccountId) {
      setupListener();
    }
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [selectedAccountId, gameSettingsSaving]);

  const loadGameSettings = async (accId: string) => {
    setGameSettingsLoading(true);
    setGameSettingsLoadError(null);
    try {
      const data = await invoke<SettingsMap>("get_account_settings", { accountId: accId });
      setGameSettings(data);
      setGameSettingsChanged(false);
    } catch (e) {
      setGameSettings({});
      setGameSettingsChanged(false);
      setGameSettingsLoadError(String(e));
      showToast("error", `加载账号游戏配置失败: ${e}`);
    } finally {
      setGameSettingsLoading(false);
    }
  };

  // Close / Rollback
  const handleClose = () => {
    if (config && installationPathEditsAreInvalid(originalConfig, config)) {
      setActiveTab("paths");
      showToast("error", "请至少保留一组国服或国际服的游戏安装目录；Battle.net 仅供国服兼容模式使用");
      return;
    }
    if (autoSaveTimerRef.current) {
      clearTimeout(autoSaveTimerRef.current);
      autoSaveTimerRef.current = null;
    }

    const closeAfterSave = async () => {
      if (config && globalHasChanges && !(await handleSaveGlobal(true))) {
        return;
      }
      if (accountHasChanges && !(await handleSaveAccount(true))) {
        return;
      }
      onClose();
    };

    void closeAfterSave();
  };

  // Global Config Save
  const handleSaveGlobal = async (quiet = false) => {
    if (!config) return true;
    if (installationPathEditsAreInvalid(originalConfig, config)) {
      if (!quiet) showToast("error", "请至少配置一组国服或国际服的游戏安装目录；存档目录仅影响画质覆盖");
      return false;
    }
    try {
      await save(config);
      setOriginalConfig(JSON.parse(JSON.stringify(config)));
      if (!quiet) showToast("success", "全局设置已成功保存");
      return true;
    } catch (e) {
      showToast("error", `保存全局设置失败: ${e}`);
      return false;
    }
  };

  // Selected Account Config Save (includes basic metadata and game settings file)
  const handleSaveAccount = async (quiet = false) => {
    if (!selectedAccountId) return true;
    setGameSettingsSaving(true);
    try {
      const acc = accounts.find(a => a.id === selectedAccountId);
      if (!acc) return false;

      // 1. Rename if modified
      if (accountNicknameDraft.trim() && accountNicknameDraft.trim() !== (acc.display_name || acc.id)) {
        const renamed = await renameAccount(selectedAccountId, accountNicknameDraft.trim());
        if (!renamed) return false;
      }

      // 2. Set Mod args if modified
      if (accountModArgsDraft !== acc.mod_args) {
        const mods = acc.mod_list || [];
        const val = accountModArgsDraft.trim();
        const newList = (val && !mods.includes(val)) ? [...mods, val] : mods;
        await updateAccountMods(selectedAccountId, val, newList);
      }

      // 3. Set Window position if modified
      if (accountWinXDraft !== acc.window_x || accountWinYDraft !== acc.window_y) {
        await invoke("set_account_window_position", {
          accountId: selectedAccountId,
          windowX: accountWinXDraft,
          windowY: accountWinYDraft,
        });
      }

      // 4. Save game settings if modified
      if (gameSettingsChanged) {
        await invoke("save_account_settings", {
          accountId: selectedAccountId,
          settings: gameSettings,
        });
        setGameSettingsChanged(false);
        await emit("account-settings-updated", { accountId: selectedAccountId });
      }

      await loadAccounts();
      const savedName = accountNicknameDraft.trim() || acc.display_name || acc.id;
      if (!quiet) showToast("success", `账号 "${savedName}" 的设置已保存`);
      return true;
    } catch (e) {
      showToast("error", `保存账号设置失败: ${e}`);
      return false;
    } finally {
      setGameSettingsSaving(false);
    }
  };

  const handleSnapshotSystemSettings = async () => {
    if (!selectedAccountId) return;
    try {
      const settings = await invoke<SettingsMap>("snapshot_system_settings_to_account", {
        accountId: selectedAccountId,
      });
      setGameSettings(settings);
      setGameSettingsLoadError(null);
      setGameSettingsChanged(false);
      await loadAccounts();
      await emit("account-settings-updated", { accountId: selectedAccountId });
      showToast("success", "已快照系统配置到当前账号");
    } catch (e) {
      showToast("error", `快照系统配置失败: ${e}`);
    }
  };

  const handleToggleAccountSettingsMode = async (accountId: string, customized: boolean) => {
    try {
      if (customized) {
        await invoke("snapshot_system_settings_to_account", { accountId });
      } else {
        await invoke("set_settings_customized", { accountId, customized: false });
      }
      await loadAccounts();
      if (accountId === selectedAccountId) {
        await loadGameSettings(accountId);
      }
      await emit("account-settings-updated", { accountId });
    } catch (e) {
      showToast("error", `切换配置模式失败: ${e}`);
    }
  };

  // Local Config Mutation helper
  const updateConfig = (updater: (c: GlobalConfig) => void) => {
    if (config) {
      const clone = { ...config };
      updater(clone);
      useGlobalConfig.setState({ config: clone });
    }
  };

  const updateGameSetting = (key: string, value: unknown) => {
    if (gameSettingsLoadError) return;
    setGameSettings(prev => ({ ...prev, [key]: value }));
    setGameSettingsChanged(true);
  };

  // Path pickers
  const pickFile = async (field: keyof GlobalConfig, title: string, extensions?: string[]) => {
    try {
      const sel = await openDialog({
        multiple: false,
        title,
        filters: extensions ? [{ name: title, extensions }] : undefined,
      });
      if (sel) {
        updateConfig(c => {
          (c as any)[field] = sel;
        });
      }
    } catch (e) {
      showToast("error", `选择文件失败: ${e}`);
    }
  };

  const pickFolder = async (field: keyof GlobalConfig, title: string) => {
    try {
      const sel = await openDialog({
        multiple: false,
        directory: true,
        title,
      });
      if (sel) {
        updateConfig(c => {
          (c as any)[field] = sel;
        });
      }
    } catch (e) {
      showToast("error", `选择目录失败: ${e}`);
    }
  };

  const toggleAudioDiagnosticRecording = async () => {
    try {
      if (audioStatus?.diagnostic_recording) {
        const path = await invoke<string | null>("stop_rune_audio_diagnostic_recording");
        setAudioStatus(previous => previous ? {
          ...previous,
          diagnostic_recording: false,
          diagnostic_recording_path: path ?? previous.diagnostic_recording_path,
        } : previous);
        if (path) showToast("success", `诊断录音已保存：${path}`);
      } else {
        const path = await invoke<string>("start_rune_audio_diagnostic_recording");
        setAudioStatus(previous => previous ? {
          ...previous,
          diagnostic_recording: true,
          diagnostic_recording_path: path,
        } : previous);
        showToast("success", "诊断录音已开始，仅录制目标 D2R 进程的声音");
      }
    } catch (error) {
      showToast("error", `切换诊断录音失败: ${error}`);
    }
  };

  const persistAudioEnabledState = async (accountId: string, enabled: boolean) => {
    const current = useGlobalConfig.getState().config;
    if (!current) return;
    const next = {
      ...current,
      rune_audio_target_account: accountId,
      rune_audio_enabled: enabled,
    };
    useGlobalConfig.setState({ config: next });
    await save(next);
    setOriginalConfig(JSON.parse(JSON.stringify(next)));
  };

  const handleAudioTargetChange = async (accountId: string) => {
    const wasEnabled = !!useGlobalConfig.getState().config?.rune_audio_enabled;
    updateConfig(next => {
      next.rune_audio_target_account = accountId;
      next.rune_audio_enabled = false;
    });
    setAudioSetupOpen(false);
    setAudioSetupName("");
    setAudioModStateLoading(true);
    try {
      const next = await invoke<AudioModSetupState>("get_audio_mod_setup_state", { accountId });
      setAudioModState(next);
      const defaults = audioSetupDefaults(next);
      setAudioSetupSource(defaults.source);
      setAudioSetupMode(defaults.mode);
      setAudioSetupName(defaults.name);
      if (wasEnabled && next.ready) {
        await persistAudioEnabledState(accountId, true);
      } else if (wasEnabled) {
        setAudioSetupOpen(true);
      }
    } catch (error) {
      showToast("error", `无法检查账号的识别 Mod：${error}`);
    } finally {
      setAudioModStateLoading(false);
    }
  };

  const handleAudioToggle = async (enabled: boolean) => {
    if (!enabled) {
      if (trackingTargetId) await persistAudioEnabledState(trackingTargetId, false);
      else updateConfig(next => { next.rune_audio_enabled = false; });
      setAudioSetupOpen(false);
      await invoke("stop_rune_audio_monitor").catch(() => {});
      return;
    }

    const accountId = trackingTargetId || initializedTrackingAccounts[0]?.id;
    if (!accountId) {
      showToast("warning", "请先初始化一个账号");
      return;
    }
    setAudioModStateLoading(true);
    try {
      const next = await invoke<AudioModSetupState>("get_audio_mod_setup_state", { accountId });
      setAudioModState(next);
      if (next.ready) {
        await persistAudioEnabledState(accountId, true);
        setAudioSetupOpen(false);
        if (next.update_required) {
          showToast("warning", "旧版识别 Mod 仍可使用；建议更新以获得即时恐怖区域识别");
        }
        if (next.running_pid && next.active_session_ready === true) {
          await invoke("start_rune_audio_monitor").catch(() => {});
        } else if (next.restart_required) {
          showToast("warning", "设置已生效，请重启该账号的游戏后开始识别");
        }
        return;
      }

      const defaults = audioSetupDefaults(next);
      setAudioSetupSource(defaults.source);
      setAudioSetupMode(defaults.mode);
      setAudioSetupName(defaults.name);
      updateConfig(current => {
        current.rune_audio_target_account = accountId;
        current.rune_audio_enabled = false;
      });
      setAudioSetupOpen(true);
    } catch (error) {
      showToast("error", `无法开启声纹识别：${error}`);
    } finally {
      setAudioModStateLoading(false);
    }
  };

  const handlePrepareAudioMod = async () => {
    const accountId = trackingTargetId || initializedTrackingAccounts[0]?.id;
    if (!accountId) return;
    if (audioSetupMode === "existing" && !audioSetupSource) {
      showToast("warning", "请选择要保留功能的现有 Mod");
      return;
    }
    if (audioSetupNameError) {
      showToast("warning", audioSetupNameError);
      return;
    }

    setAudioPreparing(true);
    setAudioPrepareProgress({
      account_id: accountId,
      phase: "starting",
      percent: 1,
      message: "正在开始准备…",
    });
    try {
      if (isAudioModUpgrade) {
        const wasEnabled = !!useGlobalConfig.getState().config?.rune_audio_enabled;
        const next = await invoke<AudioModSetupState>("upgrade_audio_mod", {
          accountId,
          sourceModName: audioSetupMode === "existing" ? audioSetupSource : null,
        });
        setAudioModState(next);
        await persistAudioEnabledState(accountId, wasEnabled);
        setAudioSetupOpen(false);
        showToast("success", `识别 Mod“${next.current_mod_name ?? audioSetupName}”已原位更新，名称和启动参数均未改变`);
        return;
      }
      const result = await invoke<AudioModPrepareResult>("prepare_audio_mod", {
        accountId,
        modName: normalizedAudioSetupName,
        sourceModName: audioSetupMode === "existing" ? audioSetupSource : null,
      });
      const next = await invoke<AudioModSetupState>("apply_audio_mod_to_account", {
        accountId,
        modName: result.mod_name,
      });
      await loadAccounts();
      setAudioModState(next);
      await persistAudioEnabledState(accountId, true);
      setAudioSetupOpen(false);
      setAudioSetupName("");
      if (next.restart_required) {
        showToast("warning", "识别 Mod 已准备完成。当前游戏需重启一次，之后会自动识别");
      } else {
        showToast("success", "声纹识别已准备完成，下次启动会自动生效");
      }
    } catch (error) {
      showToast("error", `准备识别 Mod 失败：${error}`);
    } finally {
      setAudioPreparing(false);
    }
  };

  const applyDetectedPath = (field: keyof GlobalConfig, value: string | null) => {
    if (value) {
      updateConfig(c => {
        (c as any)[field] = value;
      });
      showToast("success", "成功自动应用检测到的路径");
    } else {
      showToast("warning", "未能检测到默认路径，请手动选择");
    }
  };

  const toggleExportAccount = (accountId: string) => {
    setExportAccountIds(current => current.includes(accountId)
      ? current.filter(id => id !== accountId)
      : [...current, accountId]);
  };

  const handleExportAccounts = async () => {
    if (exportAccountIds.length === 0) {
      showToast("warning", "请至少选择一个要导出的账号");
      return;
    }
    if (!exportPlaintextRiskAcknowledged) {
      showToast("warning", "请先确认已理解明文 Token 的账号安全风险");
      return;
    }
    const date = new Date().toISOString().slice(0, 10);
    const destination = await saveDialog({
      title: "导出 D2RHub 账号",
      defaultPath: `D2RHub-accounts-${date}.json`,
      filters: [{ name: "D2RHub 账号包", extensions: ["json"] }],
    });
    if (!destination) return;

    setAccountTransferBusy("export");
    try {
      const summary = await invoke<ExportAccountsSummary>("export_accounts", {
        accountIds: exportAccountIds,
        destination,
        acknowledgePlaintextRisk: true,
      });
      showToast("success", `已导出 ${summary.account_count} 个账号`);
      showToast(
        "warning",
        summary.plaintext_token_count > 0
          ? `导出文件包含 ${summary.plaintext_token_count} 个明文 Token；任何获得文件的人都可以登录对应账号，请妥善保管并在迁移后删除`
          : "导出文件仍包含账号认证快照，请妥善保管并在迁移后删除",
      );
      setExportPickerOpen(false);
      setExportPlaintextRiskAcknowledged(false);
    } catch (error) {
      showToast("error", `导出账号失败: ${error}`);
    } finally {
      setAccountTransferBusy(null);
    }
  };

  const handleImportAccounts = async () => {
    const source = await openDialog({
      title: "选择 D2RHub 账号导出文件",
      multiple: false,
      filters: [{ name: "D2RHub 账号包", extensions: ["json"] }],
    });
    if (!source || Array.isArray(source)) return;

    setAccountTransferBusy("import");
    try {
      const summary = await invoke<ImportAccountsSummary>("import_accounts", { source });
      await loadAccounts();
      showToast(
        "success",
        `已导入 ${summary.imported.length} 个账号，本机重新加密 ${summary.reencrypted_token_count} 个 Token`,
      );
      if (summary.warnings.length > 0) {
        const extra = summary.warnings.length > 1 ? `（另有 ${summary.warnings.length - 1} 项提示）` : "";
        showToast("warning", `${summary.warnings[0]}${extra}`);
      }
    } catch (error) {
      showToast("error", `导入账号失败: ${error}`);
    } finally {
      setAccountTransferBusy(null);
    }
  };

  // Keyboard shortcut listener
  const handleShortcutKeyDown = (e: React.KeyboardEvent<HTMLInputElement>, pos: string) => {
    e.preventDefault();
    e.stopPropagation();

    const combo = parseShortcutFromKeyEvent(e);
    if (!combo) return;

    if (config) {
      let bindings: Record<string, string> = {};
      try {
        bindings = config.shortcut_bindings_json ? JSON.parse(config.shortcut_bindings_json) : {};
      } catch {
        bindings = {};
      }
      bindings[pos] = combo;
      updateConfig(c => {
        c.shortcut_bindings_json = JSON.stringify(bindings);
      });
    }

    setRecordingPos(null);
    (e.target as HTMLInputElement).blur();
    showToast("success", `快捷键已配置为: ${combo}`);
  };

  const handleClearShortcut = (pos: string) => {
    if (config) {
      let bindings: Record<string, string> = {};
      try {
        bindings = config.shortcut_bindings_json ? JSON.parse(config.shortcut_bindings_json) : {};
      } catch {
        bindings = {};
      }
      delete bindings[pos];
      updateConfig(c => {
        c.shortcut_bindings_json = JSON.stringify(bindings);
      });
      showToast("info", "快捷键已清除");
    }
  };

  // Check if global config has changes compared to original
  const globalHasChanges = config && originalConfig && JSON.stringify(config) !== JSON.stringify(originalConfig);

  // Check if account-level settings have unsaved changes
  const accountHasChanges = (() => {
    if (!selectedAccountId) return false;
    const acc = accounts.find(a => a.id === selectedAccountId);
    if (!acc) return false;
    return (
      (accountNicknameDraft.trim() && accountNicknameDraft.trim() !== (acc.display_name || acc.id)) ||
      accountModArgsDraft !== (acc.mod_args || "") ||
      accountWinXDraft !== (acc.window_x ?? null) ||
      accountWinYDraft !== (acc.window_y ?? null) ||
      gameSettingsChanged
    );
  })();

  const hasUnsavedChanges = globalHasChanges || accountHasChanges;

  const autoSaveKey = JSON.stringify({
    selectedAccountId,
    accountNicknameDraft,
    accountModArgsDraft,
    accountWinXDraft,
    accountWinYDraft,
    gameSettingsChanged,
    gameSettings,
    config,
  });

  useEffect(() => {
    if (!open || !hasUnsavedChanges || (config && installationPathEditsAreInvalid(originalConfig, config))) return;

    if (autoSaveTimerRef.current) {
      clearTimeout(autoSaveTimerRef.current);
    }

    autoSaveTimerRef.current = setTimeout(() => {
      autoSaveTimerRef.current = null;
      (async () => {
        if (config && globalHasChanges) {
          await handleSaveGlobal(true);
        }
        if (accountHasChanges) {
          await handleSaveAccount(true);
        }
      })().catch(e => showToast("error", `自动保存失败: ${e}`));
    }, 800);

    return () => {
      if (autoSaveTimerRef.current) {
        clearTimeout(autoSaveTimerRef.current);
      }
    };
  }, [open, hasUnsavedChanges, autoSaveKey]);

  // Group Tabs List
  const navTabs: { id: TabType; label: string; icon: any; desc: string }[] = [
    { id: "paths", label: "连接", icon: Folder, desc: "游戏、国服战网兼容、浏览器与存档位置" },
    { id: "accounts", label: "账号", icon: User, desc: "昵称、启动方式、窗口和画质" },
    { id: "agent", label: "启动策略", icon: Play, desc: "战网 Agent 与启动等待策略" },
    { id: "shortcuts", label: "快捷键", icon: Settings, desc: "账号窗口切换与聚焦" },
    { id: "appearance", label: "显示", icon: Palette, desc: "主题、透明度、字体和悬浮窗" },
    { id: "automation", label: "自动化", icon: ScanEye, desc: "双阶段换房、掉落声纹、统计与协议诊断" },
    { id: "pet", label: "伴随", icon: Cat, desc: "桌宠与轻量状态提示" },
    { id: "advanced", label: "维护", icon: ShieldAlert, desc: "修复、日志、重置和向导" },
  ];

  // Search filter
  const filteredTabs = navTabs;

  const selectedAccount = accounts.find(a => a.id === selectedAccountId);
  const accountRegionLabel = (region?: string | null) =>
    region === "KR" ? "亚服" : region === "NA" ? "美服" : region === "EU" ? "欧服" : region === "Global" ? "国际服" : "国服";
  const gameSubTabs: { id: typeof gameSettingsTab; label: string }[] = [
    { id: "launch", label: "启动" },
    { id: "game_display", label: "显示" },
    { id: "game_graphics", label: "图形" },
    { id: "game_audio", label: "音频" },
    { id: "game_gameplay", label: "玩法" },
    { id: "game_automap", label: "地图" },
  ];
  const saveStatusText = gameSettingsSaving
    ? "自动保存中"
    : hasUnsavedChanges
      ? "有未保存改动"
      : "已自动保存";

  return (
    <Modal open={open} onClose={handleClose} title={`设置中心 · ${saveStatusText}`} width="max-w-[1020px]" closeOnContextMenu>
      <div className="settings-center-shell flex h-[640px] max-h-[90vh] flex-col">
        <div className="settings-center-nav shrink-0">
          <div className="grid grid-cols-8 gap-2 max-[1180px]:grid-cols-4 max-[720px]:grid-cols-2">
            {filteredTabs.map((t, idx) => {
              const Icon = t.icon;
              return (
                <button
                  key={t.id}
                  onClick={() => setActiveTab(t.id)}
                  className="setting-category-tile"
                  data-active={activeTab === t.id ? "true" : "false"}
                >
                  <div className="flex items-center justify-between">
                    <span className="tile-index">{String(idx + 1).padStart(2, "0")}</span>
                    <Icon size={13} className="text-text-muted" strokeWidth={1.8} />
                  </div>
                  <span className="text-base font-bold leading-none text-text-primary">{t.label}</span>
                  <span className="micro-meta truncate">{t.desc}</span>
                </button>
              );
            })}
            {filteredTabs.length === 0 && (
              <div className="spatial-panel col-span-full px-4 py-5 text-center text-xs text-text-muted">无匹配结果</div>
            )}
          </div>
        </div>

          {/* Tab content area */}
          <div className="min-h-0 flex-1 space-y-3 overflow-y-auto py-2 pr-1">
            {/* 1. Paths Tab */}
            {activeTab === "paths" && config && (
              <div className="settings-content-grid">
                <div className="spatial-panel p-3 space-y-2">
                  <h3 className="text-xs font-bold text-text-primary">核心程序路径</h3>
                  <InstallationProfileFields
                    edition="CN"
                    config={config}
                    settingsAvailable={settingsJsonAvailable.CN}
                    detectedSavedGames={detectedPaths.cnSavedGames}
                    updateConfig={updateConfig}
                    pickFile={pickFile}
                    pickFolder={pickFolder}
                    applyDetectedPath={applyDetectedPath}
                  />
                  <InstallationProfileFields
                    edition="Global"
                    config={config}
                    settingsAvailable={settingsJsonAvailable.Global}
                    detectedSavedGames={detectedPaths.globalSavedGames}
                    updateConfig={updateConfig}
                    pickFile={pickFile}
                    pickFolder={pickFolder}
                    applyDetectedPath={applyDetectedPath}
                  />
                </div>

                <div className="spatial-panel p-3 space-y-2">
                  <h3 className="text-xs font-bold text-text-primary">国服战网/浏览器辅助路径</h3>

                  <div className="space-y-2">
                    <label className="text-xs text-text-muted block">战网进程 Agent.exe 目录</label>
                    <div className="flex gap-2">
                      <input
                        type="text"
                        value={config.program_data_agent_path}
                        readOnly
                        className="flex-1 h-8 px-3 rounded-lg bg-surface-hover text-xs border border-border-default text-text-primary"
                      />
                      <Button size="sm" onClick={() => pickFolder("program_data_agent_path", "Agent 目录")}>浏览</Button>
                      <Button size="sm" onClick={() => applyDetectedPath("program_data_agent_path", detectedPaths.agent)}>自动探测</Button>
                    </div>
                  </div>

                  <div className="space-y-2">
                    <label className="text-xs text-text-muted block">战网 Roaming AppData 目录</label>
                    <div className="flex gap-2">
                      <input
                        type="text"
                        value={config.app_data_roaming_bnet_path}
                        readOnly
                        className="flex-1 h-8 px-3 rounded-lg bg-surface-hover text-xs border border-border-default text-text-primary"
                      />
                      <Button size="sm" onClick={() => pickFolder("app_data_roaming_bnet_path", "Roaming 战网目录")}>浏览</Button>
                      <Button size="sm" onClick={() => applyDetectedPath("app_data_roaming_bnet_path", detectedPaths.roaming)}>自动探测</Button>
                    </div>
                  </div>

                  <div className="space-y-2">
                    <label className="text-xs text-text-muted block">独立隔离浏览器程序 (Edge/Chrome.exe)</label>
                    <div className="flex gap-2">
                      <input
                        type="text"
                        value={config.browser_path}
                        readOnly
                        className="flex-1 h-8 px-3 rounded-lg bg-surface-hover text-xs border border-border-default text-text-primary"
                      />
                      <Button size="sm" onClick={() => pickFile("browser_path", "chrome.exe/msedge.exe", ["exe"])}>浏览</Button>
                      <Button size="sm" onClick={() => applyDetectedPath("browser_path", detectedPaths.browser)}>自动探测</Button>
                    </div>
                  </div>

                  <div className="flex items-center justify-between pt-1">
                    <div>
                      <span className="text-sm font-semibold text-text-secondary">隔离浏览器类型</span>
                      <p className="text-2xs text-text-muted">当前选择的沙箱浏览器品牌</p>
                    </div>
                    <select
                      value={config.browser_type}
                      onChange={e => updateConfig(c => { c.browser_type = e.target.value; })}
                      className="h-7 px-2.5 rounded-lg bg-surface-hover border border-border-default text-text-primary text-xs"
                    >
                      <option value="">未指定</option>
                      <option value="chrome">Google Chrome</option>
                      <option value="edge">Microsoft Edge</option>
                    </select>
                  </div>
                </div>
              </div>
            )}

            {/* 2. Accounts & Launch Tab */}
            {activeTab === "accounts" && (
              <div className="space-y-3">
                {accounts.length === 0 ? (
                  <div className="spatial-panel py-10 text-center text-sm text-text-muted">请先在主界面点击“添加账号”创建新账号</div>
                ) : (
                  <div className="grid grid-cols-[248px_minmax(0,1fr)] gap-3 max-[880px]:grid-cols-1">
                    <div className="space-y-1.5 max-[880px]:order-2">
                      {accounts.map((a, idx) => {
                        const active = a.id === selectedAccountId;
                        const overrideEnabled = !!a.has_customized_settings;
                        return (
                          <button
                            key={a.id}
                            onClick={async () => {
                              if (a.id === selectedAccountId) return;
                              if (accountHasChanges) {
                                if (!(await handleSaveAccount(true))) return;
                              }
                              setSelectedAccountId(a.id);
                            }}
                            className="account-option w-full text-left"
                            data-selected={active ? "true" : "false"}
                          >
                            <div className="account-option-top">
                              <div className="min-w-0">
                                <span className="tile-index">{String(idx + 1).padStart(2, "0")}</span>
                                <div className="account-option-name truncate">{a.display_name || a.id}</div>
                              </div>
                              <div className="flex shrink-0 items-start gap-2">
                                <label
                                  className="option-line option-line-inline"
                                  onClick={e => e.stopPropagation()}
                                  title="覆盖游戏配置"
                                >
                                  <input
                                    type="checkbox"
                                    className="sr-only"
                                    checked={overrideEnabled}
                                    onChange={e => void handleToggleAccountSettingsMode(a.id, e.target.checked)}
                                  />
                                  <span className={overrideEnabled ? "check-box checked" : "check-box"} />
                                  <span>覆盖游戏配置</span>
                                </label>
                                <span className={a.initialized ? "account-state-dot" : "account-state-dot warn"} />
                              </div>
                            </div>
                            <div className="account-option-meta">
                              <span className="hig-badge hig-badge-neutral">{accountRegionLabel(a.region)}</span>
                              <span className={a.auth_mode === "token" ? "hig-badge hig-badge-violet" : "hig-badge hig-badge-blue"}>
                                {a.auth_mode === "token" ? "网页 Token" : "战网认证"}
                              </span>
                              {!a.initialized && <span className="hig-badge hig-badge-red">未初始化</span>}
                            </div>
                          </button>
                        );
                      })}
                    </div>

                    {selectedAccountId && selectedAccount && (
                      <div className="setting-card min-h-[340px] max-[880px]:order-1">
                        <div className="mb-3 flex flex-wrap items-start justify-between gap-3 border-b border-border-default pb-3">
                          <div className="min-w-0">
                            <p className="text-sm font-bold text-text-primary">{selectedAccount.display_name || selectedAccount.id} · 画质与启动</p>
                            <p className="micro-meta mt-1">账号字段、启动参数和游戏内配置在这里完成。</p>
                          </div>
                          <div className="flex flex-col items-end gap-2">
                            <button
                              type="button"
                              onClick={handleSnapshotSystemSettings}
                              className="control-btn h-8 px-3"
                            >
                              快照系统配置
                            </button>
                            <div className="settings-subnav">
                              {gameSubTabs.map(tab => (
                                <button
                                  key={tab.id}
                                  onClick={() => setGameSettingsTab(tab.id)}
                                  className="control-btn h-8 px-3"
                                  data-active={gameSettingsTab === tab.id ? "true" : "false"}
                                >
                                  {tab.label}
                                </button>
                              ))}
                            </div>
                          </div>
                        </div>

                        {gameSettingsTab === "launch" && (
                          <div className="space-y-3">
                            <div className="grid grid-cols-2 gap-3 max-[720px]:grid-cols-1">
                              <Input
                                label="昵称"
                                value={accountNicknameDraft}
                                onChange={e => setAccountNicknameDraft(e.target.value)}
                              />
                              <Input
                                label="Mod 参数"
                                value={accountModArgsDraft}
                                onChange={e => setAccountModArgsDraft(e.target.value)}
                                placeholder="-mod custom -txt"
                              />
                            </div>

                            <div className="grid grid-cols-2 gap-3 max-[720px]:grid-cols-1">
                              <Input
                                label="窗口 X"
                                type="number"
                                value={accountWinXDraft !== null ? accountWinXDraft : ""}
                                onChange={e => setAccountWinXDraft(e.target.value !== "" ? Number(e.target.value) : null)}
                                placeholder="默认"
                              />
                              <Input
                                label="窗口 Y"
                                type="number"
                                value={accountWinYDraft !== null ? accountWinYDraft : ""}
                                onChange={e => setAccountWinYDraft(e.target.value !== "" ? Number(e.target.value) : null)}
                                placeholder="默认"
                              />
                            </div>

                            <div className="grid grid-cols-2 gap-3 max-[720px]:grid-cols-1">
                              <div>
                                <label className="micro-meta mb-1.5 block">分辨率</label>
                                <select
                                  value={String(gameSettings["Screen Resolution (Windowed)"] ?? "1280x720")}
                                  onChange={e => updateGameSetting("Screen Resolution (Windowed)", e.target.value)}
                                  className="line-select w-full px-2.5"
                                >
                                  {["1280x720","1600x900","1920x1080","2560x1440","3840x2160"].map(r => <option key={r} value={r}>{r}</option>)}
                                </select>
                              </div>
                              <div>
                                <label className="micro-meta mb-1.5 block">FPS</label>
                                <div className="combo-input">
                                  <input
                                    type="number"
                                    min={0}
                                    max={500}
                                    list="settings-center-fps-options"
                                    value={Number(gameSettings["Framerate Target"] ?? gameSettings["Framerate Cap"] ?? 60)}
                                    onChange={e => updateGameSetting("Framerate Target", Math.max(0, Math.min(300, Number(e.target.value) || 0)))}
                                  />
                                  <datalist id="settings-center-fps-options">
                                    {[0, 30, 60, 120, 144, 240].map(f => <option key={f} value={f}>{f === 0 ? "无限制" : `${f} FPS`}</option>)}
                                  </datalist>
                                </div>
                              </div>
                            </div>
                          </div>
                        )}

                        {gameSettingsTab !== "launch" && gameSettingsLoading && (
                          <div className="space-y-3 py-4">
                            {[1, 2, 3].map(i => (
                              <div key={i} className="h-9 skeleton rounded-lg" />
                            ))}
                          </div>
                        )}

                        {gameSettingsTab !== "launch" && !gameSettingsLoading && gameSettingsLoadError && (
                          <div className="rounded-lg border border-warning/40 bg-warning/5 p-4">
                            <p className="text-sm font-semibold text-text-primary">画质配置暂不可用</p>
                            <p className="mt-1 text-xs leading-relaxed text-text-secondary">
                              {gameSettingsLoadError}。请先启动对应客户端生成系统 Settings.json，再点击“快照系统配置”或重新检查。
                            </p>
                            <button
                              type="button"
                              className="control-btn mt-3 h-8 px-3"
                              onClick={() => void loadGameSettings(selectedAccountId)}
                            >
                              重新检查
                            </button>
                          </div>
                        )}

                        {gameSettingsTab !== "launch" && !gameSettingsLoading && !gameSettingsLoadError && (
                          <div className="space-y-4">
                            {gameSettingsTab === "game_display" && (
                              <DisplaySection settings={gameSettings} update={updateGameSetting} />
                            )}
                            {gameSettingsTab === "game_graphics" && (
                              <GraphicsSection settings={gameSettings} update={updateGameSetting} />
                            )}
                            {gameSettingsTab === "game_audio" && (
                              <AudioSection settings={gameSettings} update={updateGameSetting} />
                            )}
                            {gameSettingsTab === "game_gameplay" && (
                              <GameplaySection settings={gameSettings} update={updateGameSetting} />
                            )}
                            {gameSettingsTab === "game_automap" && (
                              <AutomapSection settings={gameSettings} update={updateGameSetting} />
                            )}
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                )}
              </div>
            )}

            {/* 3. Multi-Agent Tab */}
            {activeTab === "agent" && config && (
              <div className="settings-content-grid">
                <div className="spatial-panel p-3 space-y-2">
                  <div>
                    <span className="text-sm font-semibold text-text-secondary uppercase tracking-wider block mb-1">多开 Agent 限制行为</span>
                    <p className="text-2xs text-text-muted">防止启动多个战网客户端冲突的控制策略</p>
                  </div>

                  <div className="flex gap-2">
                    {[1, 2, 3].map(mode => {
                      const label = mode === 1 ? "模式1：延时杀" : mode === 2 ? "模式2：进程数杀" : "模式3：不处理";
                      const active = (config.agent_mode ?? 1) === mode;
                      return (
                        <button
                          key={mode}
                          onClick={() => updateConfig(c => { c.agent_mode = mode; })}
                          className={`flex-1 px-3 py-2 rounded-xl text-xs font-semibold transition-all duration-150 border ${
                            active
                              ? "border-accent bg-accent/10 text-accent font-bold"
                              : "border-border-default text-text-secondary hover:text-text-primary"
                          }`}
                        >
                          {label}
                        </button>
                      );
                    })}
                  </div>

                  {/* Mode 1 Details */}
                  {(config.agent_mode ?? 1) === 1 && (
                    <div className="px-3 py-2.5 spatial-panel space-y-2">
                      <span className="text-xs text-text-muted font-medium block">检测到 Agent 后延迟杀死（秒）</span>
                      <div className="flex items-center gap-3">
                        <input
                          type="range"
                          min={0}
                          max={30}
                          step={0.1}
                          value={config.agent_delay_secs ?? 1}
                          onChange={e => updateConfig(c => { c.agent_delay_secs = parseFloat(parseFloat(e.target.value).toFixed(1)); })}
                          className="flex-1 h-1.5 rounded-full appearance-none bg-surface-hover cursor-pointer"
                        />
                        <span className="text-xs font-mono text-text-primary w-12 text-right font-bold">
                          {(config.agent_delay_secs ?? 1).toFixed(1)}s
                        </span>
                      </div>
                      <p className="text-2xs text-text-muted">在此时间内继续进行后续挂载或登录行为而不阻塞。默认 1.0s，范围 0-30s，最小粒度 0.1s</p>
                    </div>
                  )}

                  {/* Mode 2 Details */}
                  {(config.agent_mode ?? 1) === 2 && (
                    <div className="px-3 py-2.5 spatial-panel space-y-2">
                      <span className="text-xs text-text-muted font-medium block">战网客户端运行数阈值</span>
                      <div className="flex gap-2">
                        {[5, 7].map(n => {
                          const active = (config.agent_threshold ?? 5) === n;
                          return (
                            <button
                              key={n}
                              onClick={() => updateConfig(c => { c.agent_threshold = n; })}
                              className={`flex-1 px-3 py-1.5 rounded-lg text-xs font-medium transition-all duration-150 border ${
                                active
                                  ? "border-accent bg-accent/10 text-accent font-bold"
                                  : "border-border-default text-text-secondary hover:text-text-primary"
                              }`}
                            >
                              ≥ {n} 运行客户端
                            </button>
                          );
                        })}
                      </div>
                      <p className="text-2xs text-text-muted">仅当活跃战网进程数达到或超过阈值时才终结 Agent，避免多开限制发生。</p>
                    </div>
                  )}
                </div>

                <div className="spatial-panel p-3 space-y-2">
                  <h3 className="text-xs font-bold text-text-primary">辅助自动脚本选项</h3>

                  <div className="flex items-center justify-between py-1.5">
                    <div>
                      <span className="text-sm font-semibold text-text-secondary">自动关闭隔离浏览器</span>
                      <p className="text-2xs text-text-muted">在账号挂载或登录完自动关闭浏览器以释放内存</p>
                    </div>
                    <Toggle
                      checked={!!config.auto_close_browser}
                      onChange={v => updateConfig(c => { c.auto_close_browser = v; })}
                    />
                  </div>

                  <div className="flex items-center justify-between py-1.5 border-t border-border-default/50 pt-2">
                    <div>
                      <span className="text-sm font-semibold text-text-secondary">每日检查更新</span>
                      <p className="text-2xs text-text-muted">每天第一次启动多开工具时自动检测新版本</p>
                    </div>
                    <Toggle
                      checked={!!config.enable_auto_update}
                      onChange={v => updateConfig(c => { c.enable_auto_update = v; })}
                    />
                  </div>
                </div>
              </div>
            )}

            {/* 4. Window & Appearance Tab */}
            {activeTab === "appearance" && config && (
              <div className="settings-content-grid">
                <div className="spatial-panel p-3 space-y-2">
                  <h3 className="text-xs font-bold text-text-primary">界面语言</h3>
                  <div className="flex items-center justify-between py-1">
                    <div>
                      <span className="text-sm font-semibold text-text-secondary">软件界面显示语言</span>
                      <p className="text-2xs text-text-muted">软件界面显示语言，游戏内容和符文名称不受影响</p>
                    </div>
                    <select
                      value={config.app_language || "zh-CN"}
                      onChange={async e => {
                        const language = e.target.value;
                        updateConfig(c => { c.app_language = language; });
                        const cur = useGlobalConfig.getState().config;
                        if (cur) await save({ ...cur, app_language: language });
                      }}
                      className="h-8 px-2.5 rounded-lg bg-surface-hover border border-border-default text-text-primary text-xs"
                    >
                      <option value="zh-CN">中文</option>
                      <option value="en-US">English</option>
                    </select>
                  </div>
                </div>

                <div className="spatial-panel p-3 space-y-2">
                  <h3 className="text-xs font-bold text-text-primary">主程序窗口主题</h3>
                  <div className="grid grid-cols-2 gap-2.5">
                    {([
                      { id: "onyx", label: "深色主题 (Onyx)", desc: "纯黑分层与高对比文字" },
                      { id: "light", label: "浅色主题 (Light)", desc: "极简素雅明亮界面" }
                    ] as const).map(t => {
                      const active = theme === t.id;
                      return (
                        <button
                          key={t.id}
                          onClick={() => setTheme(t.id)}
                          className="flex flex-col items-start gap-1 p-3 rounded-xl border transition-all text-left"
                          style={{
                            borderColor: active ? "var(--accent)" : "var(--border-default)",
                            background: active ? "var(--surface-hover)" : "transparent"
                          }}
                        >
                          <span className={`text-md font-bold ${active ? "text-accent" : "text-text-primary"}`}>{t.label}</span>
                          <span className="text-2xs text-text-muted">{t.desc}</span>
                        </button>
                      );
                    })}
                  </div>
                </div>

                <div className="spatial-panel p-3 space-y-2">
                  <h3 className="text-xs font-bold text-text-primary">信息悬浮窗主题</h3>
                  <div className="grid grid-cols-2 gap-2.5">
                    {([
                      { id: "onyx", label: "深色悬浮窗", desc: "纯黑分层，清晰克制" },
                      { id: "light", label: "浅色悬浮窗", desc: "明亮清晰界面" }
                    ] as const).map(t => {
                      const active = (config.theme_overlay || "light") === t.id;
                      return (
                        <button
                          key={t.id}
                          onClick={async () => {
                            updateConfig(c => { c.theme_overlay = t.id; });
                            const cur = useGlobalConfig.getState().config;
                            if (cur) await save({ ...cur, theme_overlay: t.id });
                          }}
                          className="flex flex-col items-start gap-1 p-3 rounded-xl border transition-all text-left"
                          style={{
                            borderColor: active ? "var(--accent)" : "var(--border-default)",
                            background: active ? "var(--surface-hover)" : "transparent"
                          }}
                        >
                          <span className={`text-md font-bold ${active ? "text-accent" : "text-text-primary"}`}>{t.label}</span>
                          <span className="text-2xs text-text-muted">{t.desc}</span>
                        </button>
                      );
                    })}
                  </div>
                </div>

                <div className="spatial-panel p-3 space-y-2">
                  <h3 className="text-xs font-bold text-text-primary">背景不透明度 (不影响内容/文字)</h3>

                  {/* Main window opacity */}
                  <div className="space-y-1">
                    <div className="flex justify-between items-center text-xs">
                      <span className="font-semibold text-text-secondary">主界面背景不透明度</span>
                      <div className="flex items-center gap-1.5">
                        <input
                          type="number"
                          min={10}
                          max={100}
                          value={config.main_opacity ?? 95}
                          onChange={e => {
                            const val = Math.max(10, Math.min(100, parseInt(e.target.value) || 10));
                            updateConfig(c => { c.main_opacity = val; });
                          }}
                          className="w-12 h-6 px-1 rounded bg-surface-hover text-center font-mono text-xs text-text-primary border border-border-default"
                        />
                        <span className="text-text-muted">%</span>
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      <input
                        type="range"
                        min={0}
                        max={100}
                        value={percentToSlider(config.main_opacity ?? 95)}
                        onChange={e => {
                          const val = sliderToPercent(parseInt(e.target.value));
                          updateConfig(c => { c.main_opacity = val; });
                        }}
                        className="flex-1 h-1.5 rounded-full appearance-none bg-surface-hover cursor-pointer"
                      />
                    </div>
                    <p className="text-2xs text-text-muted">采用非线性映射，滑动高透明度区间更灵敏，输入框可输入真实百分比 10-100</p>
                  </div>

                  {/* Overlay opacity */}
                  <div className="space-y-1 border-t border-border-default/50 pt-3">
                    <div className="flex justify-between items-center text-xs">
                      <span className="font-semibold text-text-secondary">信息悬浮窗背景不透明度</span>
                      <div className="flex items-center gap-1.5">
                        <input
                          type="number"
                          min={10}
                          max={100}
                          value={config.overlay_opacity ?? 95}
                          onChange={e => {
                            const val = Math.max(10, Math.min(100, parseInt(e.target.value) || 10));
                            updateConfig(c => { c.overlay_opacity = val; });
                          }}
                          className="w-12 h-6 px-1 rounded bg-surface-hover text-center font-mono text-xs text-text-primary border border-border-default"
                        />
                        <span className="text-text-muted">%</span>
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      <input
                        type="range"
                        min={0}
                        max={100}
                        value={percentToSlider(config.overlay_opacity ?? 95)}
                        onChange={e => {
                          const val = sliderToPercent(parseInt(e.target.value));
                          updateConfig(c => { c.overlay_opacity = val; });
                        }}
                        className="flex-1 h-1.5 rounded-full appearance-none bg-surface-hover cursor-pointer"
                      />
                    </div>
                  </div>
                </div>

                {/* ── 字体大小 ── */}
                <div className="spatial-panel p-3 space-y-2">
                  <h3 className="text-xs font-bold text-text-primary">字体大小</h3>
                  <div className="grid grid-cols-3 gap-2">
                    {([
                      { id: "small",   label: "小",   desc: "放大 15%" },
                      { id: "default", label: "默认", desc: "放大 30%" },
                      { id: "large",   label: "大",   desc: "放大 45%" },
                    ] as const).map(({ id, label, desc }) => {
                      const fontScale = (() => {
                        try { return document.documentElement.dataset.fontScale || "default"; }
                        catch { return "default"; }
                      })();
                      const active = fontScale === id;
                      return (
                        <button
                          key={id}
                          onClick={() => {
                            document.documentElement.dataset.fontScale = id;
                            try { localStorage.setItem("d2rhub-font-scale", id); } catch {}
                            updateConfig(c => { c.font_scale = id; });
                            const cur = useGlobalConfig.getState().config;
                            if (cur) save({ ...cur, font_scale: id });
                            setFontScaleKey(k => k + 1);
                          }}
                          className={`flex flex-col items-center gap-0.5 py-2.5 px-2 rounded-xl border transition-all ${
                            active
                              ? "border-accent bg-surface-hover"
                              : "border-border-default hover:border-border-strong"
                          }`}
                        >
                          <span className={`text-md font-bold ${active ? "text-accent" : "text-text-primary"}`}>{label}</span>
                          <span className="text-2xs text-text-muted">{desc}</span>
                        </button>
                      );
                    })}
                  </div>
                </div>

                <div className="spatial-panel p-3 space-y-2">
                  <h3 className="text-xs font-bold text-text-primary">桌面悬浮窗口</h3>
                  <div className="flex items-center justify-between gap-4 py-1">
                    <div>
                      <span className="text-sm font-semibold text-text-secondary">邪恶区域播报窗口</span>
                      <p className="text-2xs text-text-muted">独立显示当前与下一轮 TZ；支持迷你模式和贴边隐藏</p>
                    </div>
                    <div className="flex shrink-0 items-center gap-2">
                      <Button
                        variant="ghost"
                        size="sm"
                        loading={windowPlacementBusy === "overlay"}
                        disabled={!config.enable_tz_overlay || windowPlacementBusy !== null}
                        onClick={() => locateWindow("overlay")}
                        title="将窗口移到主界面所在屏幕"
                      >
                        <LocateFixed size={12} />
                        定位
                      </Button>
                      <Toggle
                        checked={!!config.enable_tz_overlay}
                        ariaLabel="显示邪恶区域播报悬浮窗"
                        onChange={async v => {
                          updateConfig(c => {
                            c.enable_tz_overlay = v;
                            c.enable_overlay = v || c.enable_stats_overlay;
                          });
                          const cur = useGlobalConfig.getState().config;
                          if (cur) await save({
                            ...cur,
                            enable_tz_overlay: v,
                            enable_overlay: v || cur.enable_stats_overlay,
                          });
                          try {
                            await setAuxiliaryWindowVisible("overlay", v);
                          } catch (e) {
                            console.error("切换 TZ 播报窗口失败", e);
                          }
                        }}
                      />
                    </div>
                  </div>
                  <div className="flex items-center justify-between gap-4 border-t border-border-default/50 py-2">
                    <div>
                      <span className="text-sm font-semibold text-text-secondary">场景统计窗口</span>
                      <p className="text-2xs text-text-muted">独立显示运行账号、场景计时与符文掉落；支持贴边自动隐藏，不使用迷你模式</p>
                    </div>
                    <div className="flex shrink-0 items-center gap-2">
                      <Button
                        variant="ghost"
                        size="sm"
                        loading={windowPlacementBusy === "stats-overlay"}
                        disabled={!config.enable_stats_overlay || windowPlacementBusy !== null}
                        onClick={() => locateWindow("stats-overlay")}
                        title="将窗口移到主界面所在屏幕"
                      >
                        <LocateFixed size={12} />
                        定位
                      </Button>
                      <Toggle
                        checked={!!config.enable_stats_overlay}
                        ariaLabel="显示场景统计悬浮窗"
                        onChange={async v => {
                          updateConfig(c => {
                            c.enable_stats_overlay = v;
                            c.enable_overlay = c.enable_tz_overlay || v;
                          });
                          const cur = useGlobalConfig.getState().config;
                          if (cur) await save({
                            ...cur,
                            enable_stats_overlay: v,
                            enable_overlay: cur.enable_tz_overlay || v,
                          });
                          try {
                            await setAuxiliaryWindowVisible("stats-overlay", v);
                          } catch (e) {
                            console.error("切换统计窗口失败", e);
                          }
                        }}
                      />
                    </div>
                  </div>
                  <div className="flex items-center justify-between gap-4 border-t border-border-default/50 pt-2">
                    <p className="min-w-0 text-2xs leading-relaxed text-text-muted">
                      显示器布局变化时会自动保证窗口可见；也可以将已启用的悬浮窗统一移回当前屏幕。
                    </p>
                    <Button
                      variant="secondary"
                      size="sm"
                      className="shrink-0"
                      loading={windowPlacementBusy === "all"}
                      disabled={windowPlacementBusy !== null || (!config.enable_tz_overlay && !config.enable_stats_overlay && !config.enable_bongo_cat)}
                      onClick={recoverAllWindows}
                    >
                      <MonitorUp size={12} />
                      全部移回
                    </Button>
                  </div>
                </div>

                <div className="spatial-panel p-3 space-y-2">
                  <h3 className="text-xs font-bold text-text-primary">游戏窗口与任务栏</h3>
                  <div className="flex items-center justify-between gap-4 py-1">
                    <div>
                      <span className="text-sm font-semibold text-text-secondary">游戏实例任务栏独立</span>
                      <p className="text-2xs text-text-muted leading-relaxed">
                        为每个账号窗口设置独立任务栏标识，可分别拖曳排序。默认关闭，仅对之后启动或重新识别的游戏窗口生效。
                      </p>
                    </div>
                    <Toggle
                      checked={!!config.separate_game_taskbar_icons}
                      ariaLabel="让每个游戏账号使用独立任务栏图标"
                      onChange={value => updateConfig(current => { current.separate_game_taskbar_icons = value; })}
                    />
                  </div>
                </div>
              </div>
            )}

            {/* 5. Rune audio telemetry & Stats Tab */}
            {activeTab === "automation" && config && (
              <div className="settings-content-grid">
                <RoomRotationTestPanel
                  config={config}
                  accounts={accounts}
                  updateConfig={updateConfig}
                />
                <div className="spatial-panel p-3 space-y-2">
                  <div className="flex items-center justify-between py-1">
                    <div className="min-w-0 pr-4">
                      <span className="text-sm font-bold text-text-secondary">音频声纹自动识别</span>
                      <p className="text-2xs text-text-muted">按 D2R 进程捕获 Mod 音频；自动识别所选掉落、场景切换并完成刷图计时统计</p>
                    </div>
                    <Toggle
                      checked={isAudioEnableRequested}
                      disabled={audioPreparing || audioModStateLoading}
                      ariaLabel="启用音频声纹自动识别"
                      descriptionId="rune-audio-readiness"
                      onChange={handleAudioToggle}
                    />
                  </div>

                  <div
                    id="rune-audio-readiness"
                    className={`rounded-xl border px-3 py-3 ${
                      isAudioRecognitionActive
                        ? "border-success/25 bg-success/10"
                        : hasReadyAudioMod
                          ? "border-accent/20 bg-surface-hover"
                          : "border-warning/25 bg-warning/10"
                    }`}
                    role="status"
                    aria-live="polite"
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="flex min-w-0 items-start gap-2.5">
                        {isAudioRecognitionActive || hasReadyAudioMod
                          ? <CheckCircle2 size={16} className={`mt-0.5 shrink-0 ${isAudioRecognitionActive ? "text-success" : "text-accent"}`} />
                          : <AlertTriangle size={16} className="mt-0.5 shrink-0 text-warning" />}
                        <div className="min-w-0">
                          <p className="text-xs font-semibold text-text-primary">
                            {isAudioRecognitionActive
                              ? "声纹识别已开启"
                              : !hasInitializedAudioAccount
                                ? isAudioEnableRequested ? "开启尚未完成：初始化账号" : "先初始化一个游戏账号"
                                : !hasAudioTarget
                                  ? isAudioEnableRequested ? "开启尚未完成：选择监听账号" : "第 2 步：选择监听账号"
                                  : audioModStateLoading
                                    ? "正在检查识别 Mod"
                                    : !hasReadyAudioMod
                                      ? isAudioEnableRequested ? "开启尚未完成：准备识别 Mod" : "还差一步：准备识别 Mod"
                                      : "准备完成，可以开启识别"}
                          </p>
                          <p className="mt-0.5 text-2xs leading-relaxed text-text-secondary">
                            {isAudioRecognitionActive
                              ? audioModState?.restart_required
                                ? "配置已完成；请重启该账号的游戏，让新的 Mod 启动参数生效。"
                                : "D2RHub 会锁定所选账号的 D2R 进程，不会录制其他应用声音。"
                              : !hasInitializedAudioAccount
                                ? "声纹需要绑定一个可启动的账号。点击右侧按钮完成初始化，再回来选择账号和准备 Mod。"
                                : !hasAudioTarget
                                  ? "声音按 D2R 进程隔离捕获；先明确要统计哪个账号。"
                                  : !hasReadyAudioMod
                                    ? "D2R 需要播放极短的识别音频。D2RHub 会保留你的原 Mod，并自动生成启动参数。"
                                    : "所有前置项均已完成。点击开启后，目标游戏运行时会自动开始识别。"}
                          </p>
                        </div>
                      </div>
                      {!isAudioRecognitionActive && (
                        <Button
                          variant={hasReadyAudioMod ? "primary" : "secondary"}
                          size="sm"
                          className="shrink-0"
                          disabled={audioPreparing || audioModStateLoading}
                          onClick={() => {
                            if (!hasInitializedAudioAccount) {
                              onClose();
                              onInitializeAccount();
                              return;
                            }
                            if (!hasAudioTarget) {
                              const firstAccount = initializedTrackingAccounts[0];
                              if (firstAccount) void handleAudioTargetChange(firstAccount.id);
                              return;
                            }
                            void handleAudioToggle(true);
                          }}
                        >
                          {!hasInitializedAudioAccount
                            ? "初始化账号"
                            : !hasAudioTarget
                              ? "选择首个账号"
                              : !hasReadyAudioMod
                                ? "开始准备"
                                : "立即开启"}
                        </Button>
                      )}
                    </div>
                    <ol className="mt-3 grid grid-cols-3 gap-2" aria-label="声纹识别启用步骤">
                      {[
                        { label: "初始化账号", complete: hasInitializedAudioAccount },
                        { label: "选择监听账号", complete: hasAudioTarget },
                        { label: "准备识别 Mod", complete: hasReadyAudioMod },
                      ].map((step, index) => (
                        <li
                          key={step.label}
                          className={`flex min-w-0 items-center gap-1.5 rounded-lg px-2 py-1.5 text-2xs ${
                            step.complete ? "bg-success/10 text-success" : "bg-surface-card text-text-secondary"
                          }`}
                        >
                          <span
                            className={`grid h-4 w-4 shrink-0 place-items-center rounded-full text-[9px] font-bold ${
                              step.complete ? "bg-success text-black" : "border border-border-default text-text-muted"
                            }`}
                            aria-hidden="true"
                          >
                            {step.complete ? "✓" : index + 1}
                          </span>
                          <span className="truncate">{step.label}</span>
                        </li>
                      ))}
                    </ol>
                  </div>

                  <div className="space-y-1.5 border-t border-border-default/50 pt-3">
                    <div className="flex items-center justify-between gap-4">
                      <div>
                        <label htmlFor="rune-audio-target-account" className="text-sm font-semibold text-text-secondary">
                          识别目标账号
                        </label>
                        <p className="text-2xs text-text-muted">选择一个已初始化账号，声音只从其 D2R PID 捕获</p>
                      </div>
                      <select
                        id="rune-audio-target-account"
                        value={trackingTarget.valid ? trackingTarget.account.id : ""}
                        disabled={initializedTrackingAccounts.length === 0}
                        aria-describedby="rune-audio-target-help"
                        onChange={e => void handleAudioTargetChange(e.target.value)}
                        className="h-8 min-w-36 px-2.5 rounded-lg bg-surface-hover border border-border-default text-text-primary text-xs disabled:opacity-50 disabled:cursor-not-allowed"
                      >
                        <option value="" disabled>
                          {initializedTrackingAccounts.length === 0 ? "暂无可用账号" : "请选择账号"}
                        </option>
                        {initializedTrackingAccounts.map(account => (
                          <option key={account.id} value={account.id}>{account.display_name || account.id}</option>
                        ))}
                      </select>
                    </div>
                    <p id="rune-audio-target-help" aria-live="polite" className="text-2xs text-text-secondary">
                      {initializedTrackingAccounts.length === 0
                        ? "上方“初始化账号”会直接打开账号向导；完成后回到这里继续。"
                        : trackingTarget.valid
                          ? `只识别“${trackingTarget.account.display_name || trackingTarget.account.id}”对应的游戏声音。`
                          : "必须先选择目标账号；也可点击上方“选择首个账号”快速继续。"}
                    </p>
                  </div>

                  {trackingTarget.valid && (
                    <div className="border-t border-border-default/50 pt-3">
                      {audioModStateLoading && !audioModState ? (
                        <div className="h-16 rounded-xl bg-surface-hover skeleton" aria-label="正在检查识别 Mod" />
                      ) : audioModState?.ready && !audioSetupOpen ? (
                        <div className={`flex items-start justify-between gap-3 rounded-xl px-3 py-2.5 ${
                          audioModState.update_required ? "bg-warning/10" : "bg-success/10"
                        }`}>
                          <div className="flex min-w-0 items-start gap-2.5">
                            {audioModState.update_required
                              ? <AlertTriangle size={16} className="mt-0.5 shrink-0 text-warning" />
                              : <CheckCircle2 size={16} className="mt-0.5 shrink-0 text-success" />}
                            <div className="min-w-0">
                              <p className="text-xs font-semibold text-text-primary">
                                {audioModState.update_required ? "识别 Mod 可以更新" : "识别 Mod 已就绪"}
                              </p>
                              <p className="mt-0.5 text-2xs text-text-secondary">
                                {audioModState.update_required
                                  ? audioModState.message
                                  : `${audioModState.current_mod_name} · 启动参数已自动配置`}
                              </p>
                              {audioModState.restart_required && (
                                <p className="mt-1 text-2xs text-warning">当前游戏仍是旧配置，重启该账号后生效</p>
                              )}
                            </div>
                          </div>
                          <button
                            type="button"
                            onClick={() => {
                              const defaults = audioSetupDefaults(audioModState);
                              setAudioSetupMode(defaults.mode);
                              setAudioSetupSource(defaults.source);
                              setAudioSetupName(defaults.name);
                              setAudioSetupOpen(true);
                            }}
                            className="shrink-0 text-2xs font-medium text-text-secondary hover:text-text-primary"
                          >
                            {audioModState.update_required ? "更新" : "重新准备"}
                          </button>
                        </div>
                      ) : audioSetupOpen ? (
                        <div className="audio-mod-setup rounded-xl bg-surface-hover p-3">
                          <div className="flex items-start gap-2.5">
                            <Package size={16} className="mt-0.5 shrink-0 text-accent" />
                            <div>
                              <p className="text-xs font-semibold text-text-primary">
                                {audioModState?.update_required ? "更新识别 Mod" : "准备识别 Mod"}
                              </p>
                              <p className="mt-0.5 text-2xs leading-relaxed text-text-secondary">
                                {audioModState?.update_required
                                  ? `关闭该账号游戏后，D2RHub 会先完整生成并校验新版，再原位替换“${audioModState.current_mod_name}”；名称和启动参数不变。`
                                  : "选择准备方式并命名新 Mod，其他内容由 D2RHub 自动完成。"}
                              </p>
                            </div>
                          </div>

                          <div className="mt-3 flex gap-2" role="radiogroup" aria-label="识别 Mod 类型">
                            <button
                              type="button"
                              role="radio"
                              aria-checked={audioSetupMode === "original"}
                              disabled={audioPreparing}
                              onClick={() => setAudioSetupMode("original")}
                              className={`audio-mod-choice flex-1 ${audioSetupMode === "original" ? "is-selected" : ""}`}
                            >
                              <span className="block text-xs font-semibold">我玩原版</span>
                              <span className="mt-0.5 block text-2xs text-text-muted">只加入识别能力</span>
                            </button>
                            <button
                              type="button"
                              role="radio"
                              aria-checked={audioSetupMode === "existing"}
                              disabled={audioPreparing || !audioModState?.installed_mods.some((mod) => mod.source_eligible)}
                              onClick={() => setAudioSetupMode("existing")}
                              className={`audio-mod-choice flex-1 ${audioSetupMode === "existing" ? "is-selected" : ""}`}
                            >
                              <span className="block text-xs font-semibold">我使用 Mod</span>
                              <span className="mt-0.5 block text-2xs text-text-muted">保留原 Mod 功能</span>
                            </button>
                          </div>

                          {audioSetupMode === "existing" && (
                            <label className="mt-2 block">
                              <span className="sr-only">选择现有 Mod</span>
                              <select
                                value={audioSetupSource}
                                disabled={audioPreparing}
                                onChange={event => setAudioSetupSource(event.target.value)}
                                className="h-8 w-full rounded-lg border border-border-default bg-surface-card px-2.5 text-xs text-text-primary"
                              >
                                <option value="" disabled>请选择未经加工的原始 Mod</option>
                                {!audioModState?.installed_mods.some((mod) => mod.source_eligible) && (
                                  <option value="">未找到可用的原始 Mod</option>
                                )}
                                {audioModState?.installed_mods
                                  .filter((mod) => mod.source_eligible)
                                  .map((mod) => <option key={mod.name} value={mod.name}>{mod.name}</option>)}
                              </select>
                              {audioModState?.update_required && audioModState.build_mode === "augment" && !audioSetupSource && (
                                <span className="mt-1 block text-2xs text-warning">
                                  旧版基于其他 Mod 生成，但未能自动确定原始 Mod；请选择当时那个未经加工的 Mod，或明确改选“我玩原版”。
                                </span>
                              )}
                            </label>
                          )}

                          {isAudioModUpgrade ? (
                            <div className="mt-3 rounded-lg border border-border-default bg-surface-card px-2.5 py-2">
                              <span className="block text-2xs font-semibold text-text-muted">原位更新，Mod 名称保持不变</span>
                              <span className="mt-0.5 block truncate font-mono text-xs font-semibold text-text-primary">
                                {audioModState?.current_mod_name}
                              </span>
                            </div>
                          ) : (
                            <>
                              <label className="mt-3 block" htmlFor="audio-mod-name">
                                <span className="flex items-center justify-between gap-3 text-2xs font-semibold text-text-secondary">
                                  <span>新 Mod 名称</span>
                                  <span className="font-normal text-text-muted">必填</span>
                                </span>
                                <input
                                  id="audio-mod-name"
                                  type="text"
                                  value={audioSetupName}
                                  maxLength={AUDIO_MOD_NAME_MAX_LENGTH}
                                  disabled={audioPreparing}
                                  autoFocus
                                  autoCapitalize="off"
                                  autoCorrect="off"
                                  spellCheck={false}
                                  aria-invalid={!!audioSetupNameError}
                                  aria-describedby="audio-mod-name-help"
                                  placeholder="例如：MyAudioMod"
                                  onChange={event => setAudioSetupName(event.target.value)}
                                  className={`mt-1 h-8 w-full rounded-lg border bg-surface-card px-2.5 text-xs text-text-primary outline-none transition-colors placeholder:text-text-muted focus:border-accent disabled:cursor-not-allowed disabled:opacity-60 ${
                                    showAudioSetupNameError ? "border-danger" : "border-border-default"
                                  }`}
                                />
                              </label>
                              <p
                                id="audio-mod-name-help"
                                className={`mt-1 text-2xs leading-relaxed ${
                                  audioSetupNameError ? "font-medium text-warning" : "text-text-muted"
                                }`}
                              >
                                {audioSetupNameError
                                  ? audioSetupNameError
                                  : "仅可使用英文字母、数字、短横线和下划线。"}
                              </p>
                            </>
                          )}
                          <p className="mt-1 truncate rounded-md bg-surface-card px-2 py-1 font-mono text-2xs text-text-secondary">
                            -mod {audioSetupNameError ? "<名称>" : normalizedAudioSetupName} -txt -assettestmode 1
                          </p>

                          {audioPreparing && audioPrepareProgress && (
                            <div className="mt-3" aria-live="polite">
                              <div className="mb-1.5 flex items-center justify-between gap-3 text-2xs">
                                <span className="truncate text-text-secondary">{audioPrepareProgress.message}</span>
                                <span className="shrink-0 font-mono text-text-muted">{Math.round(audioPrepareProgress.percent)}%</span>
                              </div>
                              <div className="h-1.5 overflow-hidden rounded-full bg-surface-active">
                                <div
                                  className="h-full rounded-full bg-accent transition-[width] duration-200 ease-out"
                                  style={{ width: `${Math.max(2, audioPrepareProgress.percent)}%` }}
                                />
                              </div>
                            </div>
                          )}

                          {!!audioPrepareBlockedReason && !audioPreparing && (
                            <div
                              id="audio-prepare-blocked-reason"
                              className="mt-3 flex items-start gap-2 rounded-lg border border-warning/25 bg-warning/10 px-2.5 py-2 text-2xs leading-relaxed text-text-secondary"
                              role="status"
                            >
                              <AlertTriangle size={13} className="mt-0.5 shrink-0 text-warning" />
                              <span><strong className="text-warning">还不能开始：</strong>{audioPrepareBlockedReason}。</span>
                            </div>
                          )}

                          <Button
                            variant="primary"
                            size="md"
                            loading={audioPreparing}
                            disabled={!!audioPrepareBlockedReason}
                            aria-describedby={audioPrepareBlockedReason ? "audio-prepare-blocked-reason" : undefined}
                            onClick={handlePrepareAudioMod}
                            className="mt-3 w-full"
                          >
                            {audioPreparing
                              ? "正在准备，请勿关闭软件"
                              : audioPrepareBlockedReason
                                ? audioSetupNameError === "请输入新 Mod 名称"
                                  ? "填写 Mod 名称后即可准备"
                                  : "完成上方配置后即可准备"
                                : audioModState?.update_required ? "同名更新并替换旧版" : "一键准备并开启"}
                          </Button>
                          <p className="mt-2 text-center text-2xs leading-relaxed text-text-muted">
                            {isAudioModUpgrade
                              ? "不会拿旧版识别 Mod 再加工；会从原版或所选原始 Mod 重建，校验成功后才替换旧目录。"
                              : "不修改源 Mod；账号参数固定配置为 -mod 名称 -txt -assettestmode 1。"}
                          </p>
                        </div>
                      ) : (
                        <button
                          type="button"
                          onClick={() => {
                            if (audioModState) {
                              const defaults = audioSetupDefaults(audioModState);
                              setAudioSetupMode(defaults.mode);
                              setAudioSetupSource(defaults.source);
                              setAudioSetupName(defaults.name);
                            }
                            setAudioSetupOpen(true);
                          }}
                          className="flex w-full items-center justify-between gap-3 rounded-xl bg-surface-hover px-3 py-2.5 text-left hover:bg-surface-active"
                        >
                          <span className="flex min-w-0 items-start gap-2.5">
                            <AlertTriangle size={15} className="mt-0.5 shrink-0 text-warning" />
                            <span>
                              <span className="block text-xs font-semibold text-text-primary">需要先准备识别 Mod</span>
                              <span className="mt-0.5 block text-2xs text-text-secondary">{audioModState?.message ?? "点击开始，约一分钟完成"}</span>
                            </span>
                          </span>
                          <span className="shrink-0 text-2xs font-medium text-accent">开始</span>
                        </button>
                      )}
                    </div>
                  )}

                  <div className="space-y-2 border-t border-border-default/50 pt-3">
                    <div className="flex items-center justify-between text-xs">
                      <span className={audioStatus?.running ? "text-success" : "text-text-secondary"}>
                        {audioStatus?.running ? `正在捕获 · PID ${audioStatus.target_pid}` : "监控未运行"}
                      </span>
                      <span className="text-text-muted">数据包 {audioStatus?.decoded_packets ?? 0}</span>
                    </div>
                    <div className="grid grid-cols-4 gap-2 text-center text-2xs">
                      <div className="rounded bg-surface-hover px-2 py-1.5">
                        <span className="block text-text-muted">音频峰值</span>
                        <span className="font-mono text-text-primary">
                          {audioStatus ? audioStatus.audio_peak.toFixed(4) : "0.0000"}
                        </span>
                      </div>
                      <div className="rounded bg-surface-hover px-2 py-1.5">
                        <span className="block text-text-muted">符文</span>
                        <span className="font-mono text-text-primary">{audioStatus?.rune_events ?? 0}</span>
                      </div>
                      <div className="rounded bg-surface-hover px-2 py-1.5">
                        <span className="block text-text-muted">物品</span>
                        <span className="font-mono text-text-primary">{audioStatus?.item_events ?? 0}</span>
                      </div>
                      <div className="rounded bg-surface-hover px-2 py-1.5">
                        <span className="block text-text-muted">地点信号</span>
                        <span className="font-mono text-text-primary">{audioStatus?.scene_heartbeats ?? 0}</span>
                      </div>
                    </div>
                    {audioStatus?.last_marker && (
                      <p className="text-2xs text-success">
                        最近识别：{audioStatus.last_marker} · {((audioStatus.last_confidence ?? 0) * 100).toFixed(1)}%
                      </p>
                    )}
                    {audioStatus?.last_error && (
                      <p className="text-2xs text-danger break-all">{audioStatus.last_error}</p>
                    )}
                  </div>

                  {config.rune_audio_enabled && trackingTarget.valid && (
                    <details className="group border-t border-border-default/50 pt-3">
                      <summary className="flex cursor-pointer list-none items-center justify-between text-xs font-medium text-text-secondary">
                        诊断工具
                        <ChevronDown size={14} className="transition-transform duration-200 group-open:rotate-180" />
                      </summary>
                      <div className="mt-3 space-y-3">
                        <div className="flex items-center justify-between gap-3">
                          <div>
                            <span className="text-xs font-semibold text-text-secondary">识别阈值</span>
                            <p className="text-2xs text-text-muted">默认 0.56；没有误识别时无需调整</p>
                          </div>
                          <input
                            type="number"
                            min={0.4}
                            max={0.95}
                            step={0.01}
                            value={config.rune_audio_detection_threshold ?? 0.56}
                            onChange={event => updateConfig(c => {
                              c.rune_audio_detection_threshold = Number(event.target.value);
                            })}
                            className="h-8 w-24 rounded-lg border border-border-default bg-surface-hover px-2.5 text-xs text-text-primary"
                          />
                        </div>

                        <Button
                          variant="secondary"
                          size="md"
                          onClick={async () => {
                            try {
                              await save(config);
                              setOriginalConfig(JSON.parse(JSON.stringify(config)));
                              await invoke("restart_rune_audio_monitor");
                              showToast("success", "声纹监控已用新配置重启");
                            } catch (e) {
                              showToast("error", "重启声纹监控失败: " + e);
                            }
                          }}
                          className="w-full"
                        >
                          <RotateCw size={13} className="shrink-0" />
                          应用并重启识别
                        </Button>

                        <div className="border-t border-border-default/50 pt-3">
                          <Button
                            variant={audioStatus?.diagnostic_recording ? "danger" : "secondary"}
                            size="md"
                            disabled={!audioStatus?.running}
                            onClick={toggleAudioDiagnosticRecording}
                            className="w-full"
                          >
                            {audioStatus?.diagnostic_recording ? "停止并保存诊断录音" : "开始诊断录音"}
                          </Button>
                          <p className="mt-1 text-center text-2xs text-text-muted break-all">
                            {audioStatus?.diagnostic_recording
                              ? "正在录制目标游戏的声音并保存识别事件"
                              : audioStatus?.diagnostic_recording_path
                                ? `最近保存：${audioStatus.diagnostic_recording_path}`
                                : "仅录制目标游戏，不录制麦克风或其他应用"}
                          </p>
                        </div>
                      </div>
                    </details>
                  )}
                </div>

                <div className="space-y-3">
                  <details className="spatial-panel group overflow-hidden" open>
                    <summary className="flex cursor-pointer list-none items-center justify-between gap-3 p-4">
                      <span>
                        <span className="block text-sm font-bold text-text-primary">识别过滤器</span>
                        <span className="mt-0.5 block text-2xs text-text-muted">选择哪些已识别掉落写入统计；点击可收起</span>
                      </span>
                      <ChevronDown size={15} className="shrink-0 text-text-muted transition-transform duration-200 group-open:rotate-180" />
                    </summary>

                    <div className="border-t border-border-default/50 px-4 pb-4 pt-3">
                      <div className="space-y-4">
                        {([
                          {
                            id: "runes",
                            label: "符文",
                            value: config.rune_audio_min_rune_number ?? 1,
                            max: 33,
                            valueLabel: `#${config.rune_audio_min_rune_number ?? 1}–#33`,
                            detail: "最低编号（含）；滑到 #20 时只记录 #20–#33",
                            onChange: (value: number) => updateConfig(next => { next.rune_audio_min_rune_number = value; }),
                          },
                          {
                            id: "gems",
                            label: "宝石与骷髅",
                            value: config.rune_audio_min_gem_level ?? 1,
                            max: 5,
                            valueLabel: `${GEM_LEVELS[(config.rune_audio_min_gem_level ?? 1) - 1]}及以上`,
                            detail: "五档品质：碎裂、裂开、普通、无瑕疵、完美",
                            onChange: (value: number) => updateConfig(next => { next.rune_audio_min_gem_level = value; }),
                          },
                        ] as const).map(filter => {
                          const enabled = (config.rune_audio_tracked_categories ?? DEFAULT_TRACKING_CATEGORIES).includes(filter.id);
                          return (
                            <div key={filter.id} className={enabled ? "" : "opacity-55"}>
                              <div className="flex items-center justify-between gap-3">
                                <label className="flex cursor-pointer items-center gap-2 text-xs font-semibold text-text-secondary">
                                  <input
                                    type="checkbox"
                                    checked={enabled}
                                    onChange={event => updateConfig(next => {
                                      const current = new Set(next.rune_audio_tracked_categories ?? DEFAULT_TRACKING_CATEGORIES);
                                      if (event.target.checked) current.add(filter.id);
                                      else current.delete(filter.id);
                                      next.rune_audio_tracked_categories = TRACKING_CATEGORIES
                                        .map(item => item.id)
                                        .filter(id => current.has(id));
                                    })}
                                    className="accent-[var(--accent)]"
                                  />
                                  {filter.label}
                                </label>
                                <span className="rounded-md bg-surface-hover px-2 py-0.5 font-mono text-2xs font-semibold text-text-primary">
                                  {filter.valueLabel}
                                </span>
                              </div>
                              <input
                                type="range"
                                min={1}
                                max={filter.max}
                                step={1}
                                value={filter.value}
                                disabled={!enabled}
                                aria-label={`${filter.label}最低记录等级`}
                                onChange={event => filter.onChange(Number(event.target.value))}
                                className="tracking-filter-range mt-2 w-full"
                              />
                              <div className="mt-1 flex items-center justify-between text-2xs text-text-muted">
                                <span>{filter.detail}</span>
                                <span className="ml-3 shrink-0">1 — {filter.max}</span>
                              </div>
                            </div>
                          );
                        })}

                        <div className="border-t border-border-default/50 pt-3">
                          <p className="mb-2 text-2xs font-semibold text-text-muted">护身符 · 分别选择</p>
                          <div className="grid grid-cols-3 gap-1.5">
                            {CHARM_FILTERS.map(item => {
                              const categories = config.rune_audio_tracked_categories ?? DEFAULT_TRACKING_CATEGORIES;
                              const codes = config.rune_audio_tracked_charm_codes ?? CHARM_FILTERS.map(filter => filter.code);
                              const selected = categories.includes("charms") && codes.includes(item.code);
                              return (
                                <label
                                  key={item.code}
                                  className={`cursor-pointer rounded-lg border px-2 py-2 transition-colors ${selected
                                    ? "border-accent/40 bg-accent/5"
                                    : "border-border-default bg-surface-hover"}`}
                                >
                                  <span className="flex items-start gap-1.5">
                                    <input
                                      type="checkbox"
                                      checked={selected}
                                      onChange={event => updateConfig(next => {
                                        const currentCategories = new Set(next.rune_audio_tracked_categories ?? DEFAULT_TRACKING_CATEGORIES);
                                        const charmCodes = new Set(
                                          currentCategories.has("charms")
                                            ? next.rune_audio_tracked_charm_codes ?? CHARM_FILTERS.map(filter => filter.code)
                                            : [],
                                        );
                                        if (event.target.checked) charmCodes.add(item.code);
                                        else charmCodes.delete(item.code);
                                        next.rune_audio_tracked_charm_codes = CHARM_FILTERS
                                          .map(filter => filter.code)
                                          .filter(code => charmCodes.has(code));
                                        if (next.rune_audio_tracked_charm_codes.length > 0) currentCategories.add("charms");
                                        else currentCategories.delete("charms");
                                        next.rune_audio_tracked_categories = TRACKING_CATEGORIES
                                          .map(filter => filter.id)
                                          .filter(id => currentCategories.has(id));
                                      })}
                                      className="mt-0.5 accent-[var(--accent)]"
                                    />
                                    <span className="min-w-0">
                                      <span className="block text-2xs font-semibold leading-tight text-text-secondary">{item.label}</span>
                                      <span className="mt-0.5 block truncate text-[9px] text-text-muted">{item.detail}</span>
                                    </span>
                                  </span>
                                </label>
                              );
                            })}
                          </div>
                        </div>

                        <div className="border-t border-border-default/50 pt-3">
                          <p className="mb-2 text-2xs font-semibold text-text-muted">其他物品 · 按整项选择</p>
                          <div className="grid grid-cols-2 gap-x-3 gap-y-1">
                            {AGGREGATE_ITEM_FILTERS.map(item => {
                              const selected = (config.rune_audio_tracked_categories ?? DEFAULT_TRACKING_CATEGORIES).includes(item.id);
                              return (
                                <label key={item.id} className="flex cursor-pointer items-start gap-2 border-b border-border-default/40 py-2 last:border-b-0">
                                  <input
                                    type="checkbox"
                                    checked={selected}
                                    onChange={event => updateConfig(next => {
                                      const current = new Set(next.rune_audio_tracked_categories ?? DEFAULT_TRACKING_CATEGORIES);
                                      if (event.target.checked) current.add(item.id);
                                      else current.delete(item.id);
                                      next.rune_audio_tracked_categories = TRACKING_CATEGORIES
                                        .map(filter => filter.id)
                                        .filter(id => current.has(id));
                                    })}
                                    className="mt-0.5 accent-[var(--accent)]"
                                  />
                                  <span>
                                    <span className="block text-xs font-semibold text-text-secondary">{item.label}</span>
                                    <span className="block text-2xs leading-relaxed text-text-muted">{item.detail}</span>
                                  </span>
                                </label>
                              );
                            })}
                          </div>
                        </div>

                        <p className="border-t border-border-default/50 pt-3 text-2xs leading-relaxed text-text-muted">
                          所有掉落仅在已确认的野外或地下城场景中记录；主城、主界面和尚未识别地点时一律忽略。修改后点击左侧“应用并重启识别”。
                        </p>
                      </div>
                    </div>
                  </details>

                  <div className="spatial-panel p-4 space-y-3">
                    <div>
                      <span className="text-xs font-bold text-text-primary block mb-1">识别说明</span>
                      <p className="text-2xs text-text-muted">
                        D2RHub 只捕获所选账号的游戏声音，不读取游戏内存，也不会向游戏注入代码。
                      </p>
                    </div>
                    <p className="text-2xs text-text-secondary">
                      过滤器只决定 D2RHub 是否将已接收事件写入统计；Mod 始终包含完整识别声纹。
                    </p>
                    <p className="text-2xs text-warning">
                      声纹按基础物品代码识别；同一代码的暗金、套装或词缀无法仅凭音频区分。
                    </p>
                    <div className="border-t border-border-default/50 pt-3">
                      <Button
                        size="sm"
                        onClick={async () => {
                          try {
                            await invoke("open_stats_page");
                          } catch (e) {
                            showToast("error", `打开统计界面失败: ${e}`);
                          }
                        }}
                      >
                        <Play size={10} className="text-success fill-success" />
                        打开掉落统计图表
                      </Button>
                    </div>
                  </div>
                </div>
              </div>
            )}

            {/* 6. BongoCat Tab */}
            {activeTab === "pet" && config && (
              <div className="settings-content-grid">
                <div className="spatial-panel p-3 space-y-2">
                  <div className="flex items-center justify-between py-1">
                    <div>
                      <span className="text-sm font-bold text-text-secondary">开启桌面交互桌宠 (BongoCat)</span>
                      <p className="text-2xs text-text-muted">启用后会在桌面上方生成一只呆萌的猫咪，能实时同步你的鼠标划过与按键敲击</p>
                    </div>
                    <div className="flex shrink-0 items-center gap-2">
                      <Button
                        variant="ghost"
                        size="sm"
                        loading={windowPlacementBusy === "bongo-cat"}
                        disabled={!config.enable_bongo_cat || windowPlacementBusy !== null}
                        onClick={() => locateWindow("bongo-cat")}
                        title="将窗口移到主界面所在屏幕"
                      >
                        <LocateFixed size={12} />
                        定位
                      </Button>
                      <Toggle
                        checked={!!config.enable_bongo_cat}
                        onChange={async v => {
                          updateConfig(c => { c.enable_bongo_cat = v; });
                          const cur = useGlobalConfig.getState().config;
                          if (cur) await save({ ...cur, enable_bongo_cat: v });
                          try {
                            await setAuxiliaryWindowVisible("bongo-cat", v);
                          } catch (e) {
                            console.error("切换桌宠窗口显示失败", e);
                          }
                        }}
                      />
                    </div>
                  </div>

                  {config.enable_bongo_cat && (
                    <div className="space-y-3 border-t border-border-default/50 pt-3">
                      <div className="flex items-center justify-between">
                        <div>
                          <span className="text-sm font-semibold text-text-secondary">气泡对话框 (Chatterbox)</span>
                          <p className="text-2xs text-text-muted">猫咪是否会偶尔冒出搞笑的台词以及系统状态提示</p>
                        </div>
                        <Toggle
                          checked={!!config.bongo_cat_chatterbox}
                          onChange={v => updateConfig(c => { c.bongo_cat_chatterbox = v; })}
                        />
                      </div>

                      <div className="space-y-1">
                        <div className="flex justify-between items-center text-xs">
                          <span className="font-semibold text-text-secondary">猫咪显示缩放</span>
                          <span className="font-mono text-accent font-bold">{(config.bongo_cat_scale ?? 1.0).toFixed(1)}x</span>
                        </div>
                        <div className="flex items-center gap-2">
                          <input
                            type="range"
                            min={5}
                            max={50}
                            value={Math.round((config.bongo_cat_scale ?? 1.0) * 10)}
                            onChange={async (e) => {
                              const val = parseFloat(e.target.value) / 10;
                              updateConfig(c => { c.bongo_cat_scale = val; });
                              try {
                                await save({ ...config, bongo_cat_scale: val });
                              } catch (err) {
                                console.error("保存猫咪缩放失败", err);
                              }
                            }}
                            className="flex-1 h-1.5 rounded-full appearance-none bg-surface-hover cursor-pointer"
                          />
                        </div>
                      </div>

                      <div className="flex items-center justify-between pt-2">
                        <div>
                          <span className="text-sm font-semibold text-text-secondary">猫咪皮肤外观</span>
                          <p className="text-2xs text-text-muted">当前选择的猫咪贴图类型</p>
                        </div>
                        <select
                          value={config.bongo_cat_skin || "original"}
                          onChange={e => updateConfig(c => { c.bongo_cat_skin = e.target.value; })}
                          className="h-8 px-2.5 rounded-lg bg-surface-hover border border-border-default text-text-primary text-xs"
                        >
                          {(config.bongo_cat_unlocked_skins || ["original"]).map(skin => (
                            <option key={skin} value={skin}>{skin === "original" ? "经典原版" : skin}</option>
                          ))}
                        </select>
                      </div>
                    </div>
                  )}
                </div>
              </div>
            )}

            {/* 7. Shortcuts Tab */}
            {activeTab === "shortcuts" && config && (
              <div className="settings-content-grid">
                <div className="spatial-panel p-3 space-y-2 settings-span-full">
                  <h3 className="text-xs font-bold text-text-primary">切换聚焦快捷键配置</h3>
                  <p className="text-2xs text-text-muted mb-2">按下对应组合键可以一键聚焦并切换至指定的账号游戏窗口</p>

                  <div className="space-y-2.5 pt-1">
                    {shortcutAccounts.map((acc, index) => {
                      const posStr = String(index + 1);
                      let bindings: Record<string, string> = {};
                      try {
                        bindings = config.shortcut_bindings_json ? JSON.parse(config.shortcut_bindings_json) : {};
                      } catch {
                        bindings = {};
                      }
                      const shortcut = bindings[posStr] || "";
                      const isRecording = recordingPos === posStr;

                      return (
                        <div key={acc.id} className="flex items-center justify-between py-1">
                          <div>
                            <span className="text-sm font-semibold text-text-secondary">
                              位置 #{posStr}: {acc.display_name || acc.id}
                            </span>
                            <p className="text-2xs text-text-muted">当前一键聚焦的物理位置 #{posStr}</p>
                          </div>

                          <div className="flex items-center gap-2">
                            <input
                              type="text"
                              readOnly
                              value={isRecording ? "请按键输入组合..." : shortcut || "无"}
                              onKeyDown={e => handleShortcutKeyDown(e, posStr)}
                              onClick={() => setRecordingPos(posStr)}
                              className={`h-7 px-3 rounded-lg text-sm font-mono text-center select-none border focus:outline-none transition-all duration-150 ${
                                isRecording
                                  ? "border-accent bg-accent/10 text-accent font-bold animate-pulse"
                                  : "border-border-default bg-surface-hover text-text-primary cursor-pointer hover:border-border-strong"
                              }`}
                              style={{ width: 140 }}
                            />
                            {shortcut && (
                              <button
                                onClick={() => handleClearShortcut(posStr)}
                                aria-label={`清除位置 ${posStr} 快捷键`}
                                className="h-7 w-7 rounded-lg border border-border-default hover:border-error hover:bg-error/5 text-text-muted hover:text-error transition-all flex items-center justify-center"
                                title="清除"
                              >
                                <X size={12} />
                              </button>
                            )}
                          </div>
                        </div>
                      );
                    })}
                    {shortcutAccounts.length === 0 && (
                      <p className="text-center text-xs text-text-muted py-4">还没有账号</p>
                    )}
                  </div>
                </div>
              </div>
            )}

            {/* 8. Advanced Maintenance Tab */}
            {activeTab === "advanced" && config && (
              <div className="settings-content-grid">
                <div className="spatial-panel p-3 space-y-2 settings-span-full">
                  <h3 className="text-xs font-bold text-text-primary">高级维护</h3>


                  <div className="flex items-center justify-between py-1">
                    <div>
                      <span className="text-sm font-semibold text-text-secondary">打开系统运行日志</span>
                      <p className="text-2xs text-text-muted">查看当前多开工具的底层系统日志以供排查故障</p>
                    </div>
                    <Button
                      size="sm"
                      onClick={async () => {
                        try {
                          await invoke("open_logs_dir");
                        } catch (e) {
                          showToast("error", `打开日志失败: ${e}`);
                        }
                      }}
                    >
                      <FolderOpen size={11} className="mr-1" />
                      打开日志
                    </Button>
                  </div>

                  <div className="flex items-center justify-between py-1 border-t border-border-default/50 pt-3">
                    <div>
                      <span className="text-sm font-semibold text-text-secondary">重新配置游戏路径</span>
                      <p className="text-2xs text-text-muted">重新运行首次引导向导以修改基础路径配置</p>
                    </div>
                    <Button
                      size="sm"
                      onClick={() => {
                        onClose();
                        onReconfigure();
                      }}
                    >
                      <Settings size={11} className="mr-1" />
                      运行向导
                    </Button>
                  </div>
                </div>

                <div className="spatial-panel p-3 space-y-3 settings-span-full">
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="max-w-[68ch]">
                      <h3 className="text-xs font-bold text-text-primary">账号迁移</h3>
                      <p className="text-2xs text-text-muted mt-1 leading-relaxed">
                        导出账号元数据、独立画质配置与认证快照，不包含浏览器缓存。Token 会解密后以明文写入 JSON，导入时再使用目标设备的 Windows DPAPI 加密。
                      </p>
                    </div>
                    <div className="flex gap-2">
                      <Button
                        size="sm"
                        disabled={accounts.length === 0 || accountTransferBusy !== null}
                        onClick={() => {
                          setExportAccountIds(accounts.map(account => account.id));
                          setExportPlaintextRiskAcknowledged(false);
                          setExportPickerOpen(current => !current);
                        }}
                      >
                        <Download size={11} className="mr-1" />
                        导出账号
                      </Button>
                      <Button
                        size="sm"
                        disabled={accountTransferBusy !== null}
                        onClick={handleImportAccounts}
                      >
                        <Upload size={11} className="mr-1" />
                        {accountTransferBusy === "import" ? "导入中" : "导入账号"}
                      </Button>
                    </div>
                  </div>

                  {exportPickerOpen && (
                    <div className="border-t border-border-default/50 pt-3 space-y-3">
                      <div
                        role="alert"
                        className="flex items-start gap-2 rounded-lg px-3 py-2.5"
                        style={{ background: "var(--toast-warning-bg)", border: "1px solid var(--toast-warning-border)" }}
                      >
                        <ShieldAlert size={14} className="text-warning shrink-0 mt-0.5" />
                        <div className="min-w-0">
                          <p className="text-xs font-semibold text-text-primary">导出文件包含明文登录凭据</p>
                          <p className="text-2xs text-text-secondary mt-1 leading-relaxed max-w-[72ch]">
                            任何获得这份 JSON 的人都可以使用其中的 Token 登录你的账号。请只保存到可信位置，不要发送给其他人；迁移完成后应立即安全删除。
                          </p>
                        </div>
                      </div>
                      <div className="flex items-center justify-between gap-3">
                        <p className="text-xs font-semibold text-text-secondary">
                          选择要写入导出文件的账号 · 已选 {exportAccountIds.length}/{accounts.length}
                        </p>
                        <button
                          type="button"
                          className="text-xs text-accent hover:underline"
                          onClick={() => setExportAccountIds(
                            exportAccountIds.length === accounts.length ? [] : accounts.map(account => account.id),
                          )}
                        >
                          {exportAccountIds.length === accounts.length ? "取消全选" : "全选"}
                        </button>
                      </div>
                      <div className="grid grid-cols-2 gap-2 max-[720px]:grid-cols-1">
                        {accounts.map(account => {
                          const selected = exportAccountIds.includes(account.id);
                          return (
                            <label
                              key={account.id}
                              className="option-line min-w-0 cursor-pointer rounded-lg px-2.5 py-2"
                              style={{ background: selected ? "var(--surface-hover)" : "transparent" }}
                            >
                              <input
                                type="checkbox"
                                className="sr-only"
                                checked={selected}
                                onChange={() => toggleExportAccount(account.id)}
                              />
                              <span className={selected ? "check-box checked" : "check-box"} />
                              <span className="truncate text-xs text-text-secondary">
                                {account.display_name || account.id}
                              </span>
                            </label>
                          );
                        })}
                      </div>
                      <label className="option-line min-h-7 h-auto cursor-pointer rounded-lg px-2.5 py-2 bg-surface-hover">
                        <input
                          type="checkbox"
                          className="sr-only"
                          checked={exportPlaintextRiskAcknowledged}
                          onChange={(event) => setExportPlaintextRiskAcknowledged(event.target.checked)}
                        />
                        <span className={exportPlaintextRiskAcknowledged ? "check-box checked" : "check-box"} />
                        <span className="text-xs text-text-secondary leading-relaxed">
                          我已理解导出文件包含可直接使用的登录凭据，并会妥善保管
                        </span>
                      </label>
                      <div className="flex justify-end gap-2">
                        <Button
                          size="sm"
                          onClick={() => {
                            setExportPickerOpen(false);
                            setExportPlaintextRiskAcknowledged(false);
                          }}
                        >
                          取消
                        </Button>
                        <Button
                          variant="primary"
                          size="sm"
                          disabled={
                            exportAccountIds.length === 0
                            || !exportPlaintextRiskAcknowledged
                            || accountTransferBusy !== null
                          }
                          onClick={handleExportAccounts}
                        >
                          {accountTransferBusy === "export" ? "导出中" : `导出 ${exportAccountIds.length} 个账号（含明文）`}
                        </Button>
                      </div>
                    </div>
                  )}
                </div>
              </div>
            )}
          </div>
        </div>
    </Modal>
  );
}
