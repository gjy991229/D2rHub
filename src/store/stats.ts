import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { RuneDropEntry } from "./types";

const RUNE_NAMES: string[] = [
  "艾尔", "艾德", "特尔", "那夫", "爱斯", "伊司", "塔尔", "拉尔",
  "欧特", "书尔", "安姆", "索尔", "夏", "多尔", "海尔",
  "艾欧", "卢姆", "科", "法尔", "蓝姆", "普尔", "乌姆",
  "马尔", "伊斯特", "古尔", "伐克斯", "欧姆", "罗",
  "瑟", "贝", "乔", "查姆", "萨德",
];

/// 方言别名 → 标准名（让前端也能识别 OCR 的多种写法）
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

/// 单次符文掉落（前端追踪用，非持久化）
export interface DropEntry {
  runeName: string;
  runeNameEn: string | null;
  runeNumber: number;
  screenshotPath: string | null;  // 仅 #24+ 有值
}

export const MANUAL_FINISH_SCENE = "__d2rhub_manual_finish__";

interface StatsState {
  // ── 当前场景 ──
  currentScene: string;
  lastCombatScene: string;

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
  processOcrSceneText: (item: { text: string; is_town?: boolean }) => Promise<void>;
  /// 处理通道B 的 OCR 掉落结果（接收预匹配的符文数据）
  processOcrDrop: (item: {
    text: string;
    rune_number?: number | null;
    screenshot_path?: string | null;
    rune_name_en?: string | null;
  }) => void;
  fetchDbStats: (sceneName: string) => Promise<void>;
  removeCurrentDrop: (index: number) => void;
}

export const useStats = create<StatsState>((set, get) => ({
  currentScene: "等待识别...",
  lastCombatScene: "",
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
    const { isTiming, timerStart, currentScene, characterName, currentRunDrops } = get();
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
    const dropsPayload: RuneDropEntry[] = currentRunDrops.map((d) => ({
      rune_number: d.runeNumber,
      rune_name: d.runeName,
      rune_name_en: d.runeNameEn || null,
      screenshot_path: d.screenshotPath || null,
    }));

    try {
      if (import.meta.env.VITE_ENABLE_OCR !== "false") {
        await invoke("save_scene_record", {
          record: {
            absolute_time: absoluteTime,
            character_name: characterName || "未知角色",
            scene_name: currentScene,
            timer_seconds: seconds,
            drops: dropsPayload,
          },
        });
      }

      // 记录保存成功后，增加当前场景的本次启动场次
      const currentSessionRuns = get().sessionRuns[currentScene] || 0;
      set({
        sessionRuns: {
          ...get().sessionRuns,
          [currentScene]: currentSessionRuns + 1,
        },
      });
    } catch (e) {
      console.error("保存场景记录失败:", e);
    }

  },

  finishRunAsTown: async (townScene = MANUAL_FINISH_SCENE) => {
    const wasTiming = get().isTiming;
    const savePromise = wasTiming ? get().stopTimerAndSave() : Promise.resolve();

    // 与 OCR 识别到主城时使用相同的场景重置。状态先落地，避免数据库写入
    // 较慢时界面仍继续计时；stopTimerAndSave 已经持有本轮的完整保存快照。
    set({
      currentScene: townScene,
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

  fetchDbStats: async (sceneName: string) => {
    if (!sceneName || sceneName === "等待识别...") return;
    if (import.meta.env.VITE_ENABLE_OCR === "false") return;
    try {
      const stats: { avg_time: number, total_runs: number } | null = await invoke("get_scene_stats", { sceneName });
      // 竞态校验：如果当前场景已变（如已回城），丢弃迟到的历史数据
      if (get().currentScene !== sceneName) return;
      if (stats) {
        set({
          dbAvgTime: Math.round(stats.avg_time * 10) / 10,
          dbTotalRuns: stats.total_runs,
        });
      } else {
        set({ dbAvgTime: null, dbTotalRuns: null });
      }
    } catch {
      if (get().currentScene === sceneName) {
        set({ dbAvgTime: null, dbTotalRuns: null });
      }
    }
  },

  processOcrSceneText: async (item) => {
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
          set({ currentScene: normalized, lastCombatScene: normalized });
          get().startTimer();
          get().fetchDbStats(normalized);
        }
        // 场景没变，继续计时
      } else {
        // 从城镇/初始进入战斗
        set({ currentScene: normalized, lastCombatScene: normalized });
        get().startTimer();
        get().fetchDbStats(normalized);
      }
    }
  },

  processOcrDrop: (item) => {
    const { text, rune_number, screenshot_path, rune_name_en } = item;

    // 优先使用后端匹配的符文编号，其次前端本地匹配
    let runeName: string;
    let runeNumber: number;

    if (rune_number && rune_number >= 1 && rune_number <= 33 && RUNE_NAMES[rune_number - 1]) {
      runeNumber = rune_number;
      runeName = RUNE_NAMES[rune_number - 1];
    } else {
      const matched = matchRune(text);
      if (!matched) return;
      runeName = matched;
      runeNumber = getRuneNumber(matched);
    }

    // 每个 OCR 结果 = 一次独立掉落（支持同一符文多次掉落，各有截图）
    const newDrop: DropEntry = {
      runeName,
      runeNameEn: rune_name_en || null,
      runeNumber,
      screenshotPath: screenshot_path || null,
    };

    set({
      currentDrops: [...get().currentDrops, newDrop],
      currentRunDrops: [...get().currentRunDrops, newDrop],
    });
  },

  removeCurrentDrop: (index) => {
    set({
      currentDrops: get().currentDrops.filter((_, idx) => idx !== index),
    });
  },
}));
