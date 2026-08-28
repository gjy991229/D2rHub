import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { PersistedDropEntry, TrackingSnapshot } from "./types";

const RUNE_NAMES: string[] = [
  "艾尔", "艾德", "特尔", "那夫", "爱斯", "伊司", "塔尔", "拉尔",
  "欧特", "书尔", "安姆", "索尔", "夏", "多尔", "海尔",
  "艾欧", "卢姆", "科", "法尔", "蓝姆", "普尔", "乌姆",
  "马尔", "伊斯特", "古尔", "伐克斯", "欧姆", "罗",
  "瑟", "贝", "乔", "查姆", "萨德",
];

/// 方言别名 → 标准名（兼容旧数据库中的历史掉落名称）
const RUNE_ALIASES: Record<string, string> = {
  "提尔": "特尔",
  "奈夫": "那夫",
  "图尔": "书尔",
  "沙伊": "夏",
  "兰姆": "蓝姆",
  "玛尔": "马尔",
  "扎哈": "乔",
  "佐德": "萨德",
  "伊斯": "伊司",
  "伊司特": "伊斯特",
  "艾斯": "爱斯",
  "埃欧": "艾欧",
};

/// 获取符文编号（1-based，与数组索引对应），支持别名
export function getRuneNumber(name: string): number {
  const standard = RUNE_ALIASES[name] || name;
  return RUNE_NAMES.indexOf(standard) + 1;
}

/// 获取符文显示名称：#N 符文名
export function getRuneDisplayName(name: string): string {
  const n = getRuneNumber(name);
  return n > 0 ? `${name}#${n}` : name;
}

/// 是否为高级符文（#24 伊斯特 及以上）
export function isHighRune(runeNumber: number): boolean {
  return runeNumber >= 24;
}

export function getSessionRunKey(sceneName: string, tz: boolean): string {
  return `${tz ? "tz" : "normal"}:${sceneName}`;
}



function matchRune(text: string): string | null {
  const sanitized = text.trim().toLowerCase();

  // 合并所有候选项（标准名+别名），按长度降序避免短名抢占长名
  const candidates: { key: string; name: string }[] = [];
  for (const rune of RUNE_NAMES) {
    candidates.push({ key: rune, name: rune });
  }
  for (const [alias, standard] of Object.entries(RUNE_ALIASES)) {
    candidates.push({ key: alias, name: standard });
  }
  candidates.sort((a, b) => b.key.length - a.key.length);

  for (const { key, name } of candidates) {
    if (sanitized.includes(key)) return name;
  }
  return null;
}

/// 单次掉落（前端追踪用，非持久化）
export interface DropEntry {
  kind: "rune" | "item";
  telemetryId: number;
  itemCode: string | null;
  category: string;
  name: string;
  nameEn: string | null;
  runeNumber: number | null;
  screenshotPath: string | null;  // 仅 #24+ 有值
}

export const MANUAL_FINISH_SCENE = "__d2rhub_manual_finish__";

interface StatsState {
  // ── 当前场景 ──
  currentScene: string;
  currentTz: boolean;
  lastCombatScene: string;
  currentRunKey: string;
  currentRunName: string;
  currentRunNameEn: string;

  // ── 计时器 ──
  isTiming: boolean;
  timerStart: number | null;
  elapsedMs: number;

  // ── 数据库历史平均耗时和总场次（当前场景）──
  dbAvgTime: number | null;
  dbTotalRuns: number | null;

  // ── 本次启动各场景刷图场次 ──
  sessionRuns: Record<string, number>;

  // ── 累计掉落（悬浮窗展示，跨场景不清空）──
  currentDrops: DropEntry[];
  // ── 当前单次场景掉落（每次 startTimer 重置，仅用于数据库存储）──
  currentRunDrops: DropEntry[];

  // ── 角色昵称 ──
  characterName: string;

  // ── Actions ──
  setCharacterName: (name: string) => void;
  startTimer: () => void;
  stopTimerAndSave: () => Promise<void>;
  finishRunAsTown: (townScene?: string) => Promise<boolean>;
  tick: () => void;
  processAreaEvent: (item: { text: string; is_town?: boolean }) => Promise<void>;
  /// 处理声纹解码器产生的符文掉落事件。
  processRuneDrop: (item: {
    rune_number: number;
    rune_name?: string | null;
    rune_name_en?: string | null;
  }) => void;
  processItemDrop: (item: {
    item_id: number;
    item_code: string;
    category: string;
    item_name: string;
    item_name_en?: string | null;
  }) => void;
  applyTrackingSnapshot: (snapshot: TrackingSnapshot) => void;
  fetchDbStats: (sceneName: string, tz?: boolean) => Promise<void>;
  removeCurrentDrop: (index: number) => void;
}

export const useStats = create<StatsState>((set, get) => ({
  currentScene: "等待识别...",
  currentTz: false,
  lastCombatScene: "",
  currentRunKey: "",
  currentRunName: "",
  currentRunNameEn: "",
  isTiming: false,
  timerStart: null,
  elapsedMs: 0,
  dbAvgTime: null,
  dbTotalRuns: null,
  sessionRuns: {},
  currentDrops: [],
  currentRunDrops: [],
  characterName: "",

  setCharacterName: (name) => set({ characterName: name }),

  startTimer: () => {
    set({ isTiming: true, timerStart: Date.now(), elapsedMs: 0, currentRunDrops: [] });
  },

  stopTimerAndSave: async () => {
    const { isTiming, timerStart, currentScene, currentTz, currentRunName, characterName, currentRunDrops } = get();
    if (!isTiming || !timerStart) return;

    const elapsed = Date.now() - timerStart;
    const seconds = Math.round(elapsed / 100) / 10;

    // 先同步冻结计时状态，再异步保存快照。这样手动结束会立即反馈，保存期间
    // 新识别到的场景也不会被迟到的旧保存流程重置。
    set({ isTiming: false, timerStart: null, elapsedMs: 0, currentRunDrops: [] });

    const now = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    const absoluteTime = `${now.getFullYear()}/${pad(now.getMonth() + 1)}/${pad(now.getDate())}/${pad(now.getHours())}:${pad(now.getMinutes())}:${pad(now.getSeconds())}`;

    // 仅使用当前单次场景的掉落（非累计），发送到后端存储
    const dropsPayload: PersistedDropEntry[] = currentRunDrops.map((d) => ({
      kind: d.kind,
      telemetry_id: d.telemetryId,
      item_code: d.itemCode,
      category: d.category,
      display_name: d.name,
      display_name_en: d.nameEn || null,
      rune_number: d.runeNumber,
      screenshot_path: d.screenshotPath || null,
    }));

    try {
      await invoke("save_scene_record", {
        record: {
          absolute_time: absoluteTime,
          character_name: characterName || "未知角色",
          scene_name: currentRunName || currentScene,
          tz: currentTz,
          timer_seconds: seconds,
          drops: dropsPayload,
        },
      });

      // 记录保存成功后，增加当前场景的本次启动场次
      const sessionKey = getSessionRunKey(currentRunName || currentScene, currentTz);
      const currentSessionRuns = get().sessionRuns[sessionKey] || 0;
      set({
        sessionRuns: {
          ...get().sessionRuns,
          [sessionKey]: currentSessionRuns + 1,
        },
      });
    } catch (e) {
      console.error("保存场景记录失败:", e);
    }

  },

  finishRunAsTown: async (townScene = MANUAL_FINISH_SCENE) => {
    const wasTiming = get().isTiming;
    const savePromise = wasTiming ? get().stopTimerAndSave() : Promise.resolve();

    // 与音频遥测识别到主城时使用相同的场景重置。状态先落地，避免数据库写入
    // 较慢时界面仍继续计时；stopTimerAndSave 已经持有本轮的完整保存快照。
    set({
      currentScene: townScene,
      currentTz: false,
      lastCombatScene: "",
      dbAvgTime: null,
      dbTotalRuns: null,
    });
    await savePromise;
    return wasTiming;
  },

  tick: () => {
    const { isTiming, timerStart } = get();
    if (!isTiming || !timerStart) return;
    set({ elapsedMs: Date.now() - timerStart });
  },

  fetchDbStats: async (sceneName: string, tz = false) => {
    if (!sceneName || sceneName === "等待识别...") return;
    try {
      const stats: { avg_time: number, total_runs: number } | null = await invoke("get_scene_stats", { sceneName, tz });
      // 竞态校验：如果当前场景已变（如已回城），丢弃迟到的历史数据
      if (
        (get().currentRunName !== sceneName && get().currentScene !== sceneName)
        || get().currentTz !== tz
      ) return;
      if (stats) {
        set({
          dbAvgTime: Math.round(stats.avg_time * 10) / 10,
          dbTotalRuns: stats.total_runs,
        });
      } else {
        set({ dbAvgTime: null, dbTotalRuns: null });
      }
    } catch {
      if (
        (get().currentRunName === sceneName || get().currentScene === sceneName)
        && get().currentTz === tz
      ) {
        set({ dbAvgTime: null, dbTotalRuns: null });
      }
    }
  },

  processAreaEvent: async (item) => {
    let normalized = item.text.trim();
    if (!normalized) return;

    // 清理场景名称：删除"进入"前缀及多余空白
    normalized = normalized.replace(/^进入\s*/, "").trim();
    if (!normalized) return;

    const { isTiming, currentScene } = get();
    const isTown = item.is_town || false;

    if (isTown) {
      // 回城：保存当前战斗场景，再进入统一的主城状态。
      await get().finishRunAsTown(normalized);
    } else {
      // 战斗场景
      if (isTiming) {
        if (normalized !== currentScene) {
          // 场景切换：保存旧场景 → 开始新场景
          await get().stopTimerAndSave();
          set({ currentScene: normalized, currentTz: false, lastCombatScene: normalized });
          get().startTimer();
          get().fetchDbStats(normalized);
        }
        // 场景没变，继续计时
      } else {
        // 从城镇/初始进入战斗
        set({ currentScene: normalized, currentTz: false, lastCombatScene: normalized });
        get().startTimer();
        get().fetchDbStats(normalized);
      }
    }
  },

  processRuneDrop: (item) => {
    const { rune_number, rune_name, rune_name_en } = item;

    // 优先使用后端匹配的符文编号，其次前端本地匹配
    let runeName: string;
    let runeNumber: number;

    if (rune_number >= 1 && rune_number <= 33 && RUNE_NAMES[rune_number - 1]) {
      runeNumber = rune_number;
      runeName = RUNE_NAMES[rune_number - 1];
    } else {
      const matched = rune_name ? matchRune(rune_name) : null;
      if (!matched) return;
      runeName = matched;
      runeNumber = getRuneNumber(matched);
    }

    // 每个解码器确认的播放事件 = 一次独立掉落观测；不施加业务冷却。
    const newDrop: DropEntry = {
      kind: "rune",
      telemetryId: runeNumber,
      itemCode: `r${String(runeNumber).padStart(2, "0")}`,
      category: "runes",
      name: runeName,
      nameEn: rune_name_en || null,
      runeNumber,
      screenshotPath: null,
    };

    set({
      currentDrops: [...get().currentDrops, newDrop],
    });
  },

  processItemDrop: (item) => {
    if (!item.item_id || !item.item_code || !item.item_name) return;
    const newDrop: DropEntry = {
      kind: "item",
      telemetryId: item.item_id,
      itemCode: item.item_code,
      category: item.category,
      name: item.item_name,
      nameEn: item.item_name_en || null,
      runeNumber: null,
      screenshotPath: null,
    };
    set({ currentDrops: [...get().currentDrops, newDrop] });
  },

  applyTrackingSnapshot: (snapshot) => {
    const previousRunName = get().currentRunName;
    const previousTz = get().currentTz;
    const currentRunDrops: DropEntry[] = snapshot.current_run_drops.map((drop) => ({
      kind: drop.kind,
      telemetryId: drop.telemetry_id,
      itemCode: drop.code || null,
      category: drop.category,
      name: drop.name,
      nameEn: drop.name_en || null,
      runeNumber: drop.rune_number || null,
      screenshotPath: null,
    }));
    set({
      currentScene: snapshot.current_scene || "等待识别...",
      currentTz: Boolean(snapshot.tz),
      lastCombatScene: snapshot.location_kind === "wilderness" ? snapshot.current_scene : "",
      currentRunKey: snapshot.current_run_key || "",
      currentRunName: snapshot.current_run_name || "",
      currentRunNameEn: snapshot.current_run_name_en || "",
      isTiming: snapshot.is_timing,
      timerStart: snapshot.timer_started_at_ms,
      elapsedMs: snapshot.is_timing && snapshot.timer_started_at_ms
        ? Math.max(0, Date.now() - snapshot.timer_started_at_ms)
        : 0,
      currentRunDrops,
      sessionRuns: snapshot.session_runs,
      ...(snapshot.is_timing ? {} : { dbAvgTime: null, dbTotalRuns: null }),
    });
    if (
      snapshot.current_run_name
      && (snapshot.current_run_name !== previousRunName || Boolean(snapshot.tz) !== previousTz)
    ) {
      void get().fetchDbStats(snapshot.current_run_name, Boolean(snapshot.tz));
    }
  },

  removeCurrentDrop: (index) => {
    set({
      currentDrops: get().currentDrops.filter((_, idx) => idx !== index),
    });
  },
}));
