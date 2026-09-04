import type { OptionalModuleTabId, SettingsLanguage } from "../settings/settingsRegistry";

export interface DisclosureSection {
  title: string;
  body: string;
  tone?: "warning";
}

export interface DisclosureCopy {
  title: string;
  context: string;
  summary: string;
  sections: readonly DisclosureSection[];
  cancel: string;
  accept: string;
  waiting: string;
}

const APPLICATION_COPY: Record<SettingsLanguage, Omit<DisclosureCopy, "context">> = {
  "zh-CN": {
    title: "使用须知与免责声明",
    summary: "请在使用 D2RHub 前了解它的工作方式、系统权限与账号风险。",
    sections: [
      {
        title: "软件说明",
        body: "D2RHub 是面向《暗黑破坏神 II：重制版》（D2R）的 Windows 本地开源工具，用于管理多个账号与游戏实例，并提供窗口切换、悬浮信息、识别统计等功能。本软件并非暴雪官方产品，与 Blizzard Entertainment 无隶属或合作关系。",
      },
      {
        title: "多开原理",
        body: "D2RHub 通过管理员权限切换并隔离不同账号的本机认证信息和游戏设置，逐个启动 D2R；同时查找并关闭目标游戏进程的多开限制句柄，并监听系统事件以确认认证 Token 已被读取。程序不会向游戏注入 DLL、读写游戏内存或修改游戏可执行文件。",
      },
      {
        title: "风险提示",
        tone: "warning",
        body: "相关功能涉及进程管理、Battle.net 认证注册表读写、加密 Token 本地保存及游戏进程句柄操作，可能被安全软件或反作弊系统拦截，也可能因系统、Battle.net 或游戏更新造成登录、启动、配置或数据异常。暴雪可能将有关行为认定为未经授权的第三方软件使用，并对账号采取警告、暂停或封禁等措施。",
      },
      {
        title: "免责声明",
        body: "你应自行判断相关功能是否符合所在地法律及暴雪最新规则，并自行承担使用风险。本软件按现状提供，不保证始终可用、准确、安全、兼容或不会导致账号处罚。请提前备份重要数据；在法律允许范围内，开发者及贡献者不对账号处罚、数据丢失、服务中断或由使用本软件造成的其他损失承担责任。",
      },
    ],
    cancel: "退出 D2RHub",
    accept: "我已了解并接受",
    waiting: "请阅读以上内容，接受按钮将在倒计时结束后启用",
  },
  "en-US": {
    title: "Notice and Disclaimer",
    summary: "Before using D2RHub, review how it works, the system access it uses, and the account risks involved.",
    sections: [
      {
        title: "About",
        body: "D2RHub is an open-source local Windows tool for managing multiple Diablo II: Resurrected accounts and game instances, with optional window, overlay, recognition, and statistics features. It is not an official Blizzard product and is not affiliated with or endorsed by Blizzard Entertainment.",
      },
      {
        title: "How multi-instance works",
        body: "With administrator access, D2RHub switches and isolates local authentication data and game settings for each account, starts D2R instances in sequence, closes the target process's multi-instance restriction handle, and watches system events to confirm that the authentication token was read. It does not inject DLLs, read or write game memory, or modify the game executable.",
      },
      {
        title: "Risks",
        tone: "warning",
        body: "These operations involve process management, Battle.net authentication registry changes, encrypted local token storage, and game-process handle access. Security or anti-cheat systems may block or flag them, and system, Battle.net, or game updates may cause login, launch, configuration, or data problems. Blizzard may treat this activity as unauthorized third-party software use and may warn, suspend, or close accounts.",
      },
      {
        title: "Disclaimer",
        body: "You are responsible for deciding whether your use complies with local law and Blizzard's current rules. The software is provided as-is, without guarantees of availability, accuracy, safety, compatibility, or freedom from account action. Back up important data. To the extent permitted by law, the developers and contributors are not liable for account action, data loss, service interruption, or other losses arising from use.",
      },
    ],
    cancel: "Exit D2RHub",
    accept: "I understand and accept",
    waiting: "Review the notice above. The accept button will unlock when the countdown ends.",
  },
};

const MODULE_COPY: Record<SettingsLanguage, Record<OptionalModuleTabId, Omit<DisclosureCopy, "context">>> = {
  "zh-CN": {
    overlays: {
      title: "添加“桌面悬浮窗”前请了解",
      summary: "此说明仅在你第一次添加本模块时出现。",
      sections: [
        {
          title: "功能与原理",
          body: "本模块通过 D2RHub 自身的置顶窗口显示当前及下一轮邪恶区域、场景统计等信息。邪恶区域数据来自公开网络接口，统计数据来自本机 D2RHub；悬浮窗不会注入游戏或读取游戏内存。",
        },
        {
          title: "风险与免责",
          tone: "warning",
          body: "网络接口或本地统计可能延迟、中断或出现误差；置顶、贴边、鼠标穿透及窗口定位也可能在部分显示环境下工作异常。请以游戏内实际信息为准。在法律允许范围内，因信息错误、功能不可用或窗口行为造成的后果与损失由使用者自行承担。",
        },
      ],
      cancel: "暂不添加",
      accept: "我已了解并添加",
      waiting: "接受按钮将在倒计时结束后启用",
    },
    pet: {
      title: "添加“桌宠”前请了解",
      summary: "此说明仅在你第一次添加本模块时出现。",
      sections: [
        {
          title: "功能与原理",
          body: "本模块显示一个桌面伴随角色，根据键盘、鼠标操作播放动画并展示轻量状态提示。它使用 Windows 全局输入钩子识别按键类型和鼠标按钮，但不会保存输入文字、密码或输入历史，也不会上传输入事件。",
        },
        {
          title: "风险与免责",
          tone: "warning",
          body: "全局输入钩子可能被安全软件拦截，并可能与部分输入工具、快捷键软件或安全策略冲突；桌宠窗口也会占用少量系统资源并可能遮挡其他内容。在法律允许范围内，因兼容问题、误报、性能影响或窗口行为造成的后果与损失由使用者自行承担。",
        },
      ],
      cancel: "暂不添加",
      accept: "我已了解并添加",
      waiting: "接受按钮将在倒计时结束后启用",
    },
    automation: {
      title: "添加“识别与统计”前请了解",
      summary: "添加本模块时会同时添加“桌面悬浮窗”，并开启场景统计与邪恶区域播报。",
      sections: [
        {
          title: "功能与原理",
          body: "本模块可识别游戏场景、掉落和邪恶区域事件，并在本机记录场次与统计。它会为指定账号生成或加工 D2R Mod，由 Mod 播放极短的事件声纹，再通过 Windows 应用音频捕获识别目标 D2R 进程的声音；不会使用麦克风、注入游戏或读取游戏内存。",
        },
        {
          title: "风险与免责",
          tone: "warning",
          body: "使用自定义 Mod 和第三方识别工具可能存在兼容、误识别、漏记、性能及账号处罚风险，也可能在游戏更新后失效。诊断录音仅在你主动开启时保存到本机。是否符合暴雪规则由你自行判断；在法律允许范围内，因账号处罚、Mod 冲突、统计错误或数据损失造成的后果由使用者自行承担。",
        },
      ],
      cancel: "暂不添加",
      accept: "我已了解并添加",
      waiting: "接受按钮将在倒计时结束后启用",
    },
    "room-automation": {
      title: "添加“自动跟房”前请了解",
      summary: "此模块会代替你向指定的游戏窗口发送房间操作按键。",
      sections: [
        {
          title: "功能与原理",
          body: "本模块可让主账号自动创建或重开房间，并让跟随账号同时或按固定间隔派发进房指令。程序通过识别对应的 D2R 窗口并发送键盘消息，自动填写房间名和密码；为支持后台输入，在你同意后会先备份兼容角色原有的聊天第二快捷键，再将该槽位替换为 F13，恢复时还原原值。",
        },
        {
          title: "风险与免责",
          tone: "warning",
          body: "游戏延迟、界面状态、窗口识别或版本更新可能造成输入错误、进入错误房间或流程失败；键位文件也可能发生冲突或恢复失败。自动化行为可能被暴雪认定为违反规则并导致账号处罚。启用前请核对账号、窗口和配置；在法律允许范围内，相关账号、游戏内及配置后果与损失由使用者自行承担。",
        },
      ],
      cancel: "暂不添加",
      accept: "我已了解并添加",
      waiting: "接受按钮将在倒计时结束后启用",
    },
  },
  "en-US": {
    overlays: {
      title: "Before adding Desktop Overlays",
      summary: "This notice appears only the first time you add this module.",
      sections: [
        {
          title: "What it does",
          body: "This module uses D2RHub's own always-on-top windows to show current and upcoming Terror Zones and local run statistics. Terror Zone data comes from a public network service, while statistics come from local D2RHub data. The overlays do not inject into the game or read game memory.",
        },
        {
          title: "Risks and disclaimer",
          tone: "warning",
          body: "Network and local statistics may be delayed, unavailable, or inaccurate. Always-on-top, edge docking, click-through, and window placement may also behave unexpectedly on some displays. Treat the game as the source of truth. To the extent permitted by law, you accept consequences and losses caused by inaccurate data, unavailable features, or window behavior.",
        },
      ],
      cancel: "Not now",
      accept: "Understand and add",
      waiting: "The add button will unlock when the countdown ends.",
    },
    pet: {
      title: "Before adding Desktop Companion",
      summary: "This notice appears only the first time you add this module.",
      sections: [
        {
          title: "What it does",
          body: "This module shows a desktop companion that reacts to keyboard and mouse activity and displays lightweight status messages. It uses Windows global input hooks to identify key types and mouse buttons, but does not save typed text, passwords, or input history, and does not upload input events.",
        },
        {
          title: "Risks and disclaimer",
          tone: "warning",
          body: "Security software may block or flag global input hooks, and they may conflict with some input tools, shortcut utilities, or security policies. The companion uses a small amount of system resources and may cover other content. To the extent permitted by law, you accept consequences and losses caused by compatibility issues, false positives, performance, or window behavior.",
        },
      ],
      cancel: "Not now",
      accept: "Understand and add",
      waiting: "The add button will unlock when the countdown ends.",
    },
    automation: {
      title: "Before adding Recognition & Stats",
      summary: "Adding this module also adds Desktop Overlays and turns on statistics and Terror Zone reporting.",
      sections: [
        {
          title: "What it does",
          body: "This module recognizes scenes, drops, and Terror Zone events and stores run history locally. It creates or processes a D2R Mod that plays very short event audio markers, then uses Windows application audio capture to recognize sound from the selected D2R process. It does not use the microphone, inject into the game, or read game memory.",
        },
        {
          title: "Risks and disclaimer",
          tone: "warning",
          body: "Custom Mods and third-party recognition tools may cause compatibility problems, missed or incorrect detections, performance issues, account action, or failure after a game update. Diagnostic audio is saved locally only when you start it. You decide whether use complies with Blizzard's rules and, to the extent permitted by law, accept account, Mod, statistics, and data-loss consequences.",
        },
      ],
      cancel: "Not now",
      accept: "Understand and add",
      waiting: "The add button will unlock when the countdown ends.",
    },
    "room-automation": {
      title: "Before adding Room Automation",
      summary: "This module sends room-operation keystrokes to the selected game windows on your behalf.",
      sections: [
        {
          title: "What it does",
          body: "This module can create or recreate a room on the primary account and dispatch follower join commands together or at a fixed interval. It identifies each D2R window and sends keyboard messages to enter the room name and password. With your consent, it first backs up each compatible character's existing secondary Chat key, then replaces that slot with F13 for background entry and restores the original value when requested.",
        },
        {
          title: "Risks and disclaimer",
          tone: "warning",
          body: "Latency, interface state, window detection, or game updates may cause incorrect input, entry into the wrong room, or workflow failure. Key-binding files may conflict or fail to restore. Blizzard may treat automation as a rules violation and take account action. Verify accounts, windows, and settings before use; to the extent permitted by law, you accept related account, in-game, and configuration consequences and losses.",
        },
      ],
      cancel: "Not now",
      accept: "Understand and add",
      waiting: "The add button will unlock when the countdown ends.",
    },
  },
};

export function applicationDisclosureCopy(language: SettingsLanguage, version: string | null): DisclosureCopy {
  return {
    ...APPLICATION_COPY[language],
    context: version && version !== "unknown" ? `D2RHub v${version}` : "D2RHub",
  };
}

export function moduleDisclosureCopy(language: SettingsLanguage, module: OptionalModuleTabId): DisclosureCopy {
  return {
    ...MODULE_COPY[language][module],
    context: language === "en-US" ? "Optional module" : "可选模块",
  };
}
