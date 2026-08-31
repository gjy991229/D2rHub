<div align="center">
  <img src="logo.png" alt="D2RHub Logo" width="112" />
  <h1>D2RHub</h1>
  <p><strong>Diablo II: Resurrected 多账号、双客户端与音频遥测刷图助手</strong></p>
  <p>
    <img src="https://img.shields.io/badge/version-0.9.8-d5a85a" alt="Version 0.9.8" />
    <img src="https://img.shields.io/badge/platform-Windows_11-blue" alt="Windows 11" />
    <img src="https://img.shields.io/badge/game_memory-no_injection-2f855a" alt="No game-memory injection" />
    <img src="https://img.shields.io/badge/license-MIT-lightgrey" alt="MIT License" />
  </p>
</div>

---

## 中文

D2RHub 是一款 Windows 本地工具，用于管理《暗黑破坏神 II：重制版》的多个账号、国服/国际服客户端配置和多开流程，并通过 mod 音频声纹记录符文掉落和本地统计数据。

当前只维护一个功能完整的桌面版本，账号管理、音频声纹、统计、悬浮窗和桌宠均默认包含。MSI 与 NSIS 只是安装格式不同，不是两个功能版本。

### 当前能力

- **国服 / 国际服双客户端隔离**：分别配置游戏、存档和 Battle.net 路径；亚服、美服、欧服共用国际服档案。
- **两种账号认证**：网页 Token 直启，或 Battle.net 客户端认证与本地运行快照；Token 使用 Windows DPAPI 加密保存。
- **多账号启动控制**：单账号、启动台默认启动、全局待机池、可持久化启动方案、取消队列、运行状态与启动日志；待机池账号不参与默认启动，启动方案始终按自己的成员和账号卡片当前顺序启动。
- **账号独立游戏配置**：图形化编辑显示、图形、音频、玩法和地图设置。
- **音频数据包**：按 v7 协议识别 33 个符文、50 个扩展物品、全部游戏 Area 与主界面，并按目标 D2R PID 捕获实际混音输出；可设置最低记录符文编号，且掉落仅在已确认的野外/地下城场景入库。隐藏场景统计悬浮窗不会停止已开启的声纹监控。
- **一键准备识别 Mod**：D2RHub 内置独立的 `d2r-audio-mod.exe` 生成器。无论从原版还是现有 Mod 准备，用户都需要为新 Mod 命名；软件随后生成新 Mod、复核清单并安全配置账号启动参数。r7 配方会额外加入常驻的局内“下一局 / 创建房间 / 加入房间”工具栏：下一局先显示 4 秒确认条；房间表单在打开期间使用局内聊天层级的输入优先级，只让当前焦点框接收按键，并显式还原原生单行输入样式；可用同键、Esc 或关闭按钮退出；仍复用游戏的密码缓存与回车提交，不复制第三方 Mod 贴图。游戏、源 Mod 与输出目录支持中文、空格及 Windows 允许的特殊字符；源 Mod 数据表兼容 UTF-8、UTF-16、GBK/GB18030 与常见 Windows ANSI 编码，JSON 资源兼容常见 JSON5 写法，0 字节 FLAC 静音占位也会保持静音。现有 Mod 只需包含自己改动过的数据表，缺失表会逐个从本机游戏数据补齐。生成器代码仍在独立仓库，不读取 D2RHub 配置或数据库，也不修改源 Mod。
- **局内双阶段换房**：主号快捷键直接点击局内创建、填写递增房名并提交；确认主号进房后，小号快捷键会让全部小号从各自当前房间并行加入。全程不经过 Esc、保存退出或大厅；密码只在进程首次使用或配置改变时重填。所有参与账号必须启用同一代 r7 加工 Mod。
- **自动刷图统计**：每个不同野外独立计时，主城和主界面停止并结算；统计页可同时勾选并编辑女伯爵、地穴、安达利尔、墨菲斯托、Chaos、巴尔等常用 Farm 策略组，也可创建和编辑自定义路线。策略组内分段先合计耗时，再作为一场参与场次与平均耗时计算；重叠策略不会重复计数，原始数据保持不变。筛选器可折叠，离群优化默认开启，短空场阈值默认 1 秒且可调。
- **分组掉落反馈**：场景统计悬浮窗按物品分组显示重复掉落并标注数量；新掉落会短时弹出提示，列表默认保留最近 5 种，可按需展开全部，并支持贴边自动隐藏。
- **快捷键与桌宠**：按账号位置聚焦游戏窗口；Bongo Cat 支持缩放、气泡和可解锁皮肤。

### 快速开始

1. 从 [Releases](https://github.com/gjy991229/D2RHub/releases) 下载 MSI / NSIS 安装包。
2. 首次运行时至少完整配置一套“游戏目录 + 存档目录”。
3. 添加并初始化账号，然后从账号卡片启动单个账号、默认启动启动台账号，或创建可重复使用的启动方案；暂时不用的账号可拖入底部待机 Dock，悬浮或点击紧凑条目即可检视完整卡片。
4. 在 D2RHub 选择已初始化账号并开启音频声纹识别。首次开启时选择“我玩原版”或要保留功能的现有 Mod，并为生成的新 Mod 输入名称，然后点击“一键准备并开启”。名称仅可使用英文字母、数字、短横线和下划线。
5. 软件自动生成新的识别 Mod，并在保留其他无关启动参数的前提下将 Mod 参数固定为 `-mod <名称> -txt -assettestmode 1`。若游戏已经运行，重启该账号后生效。

默认 v7 协议使用相互隔离的地点/掉落同步码、127 chip 掉落 Gold 签名、双份数据包和 CRC-6；10 位 ID 可覆盖 Area 1–1023，默认声纹增益为 `-30dBFS`。D2RHub 内的实现与测试见 [音频遥测 v7](docs/AUDIO_TELEMETRY_V7.md)，生产端的正式协议文档随独立 Mod 工具仓库维护。

### 数据与安全边界

D2RHub 通过 Windows 进程、句柄、注册表、文件、窗口和 WASAPI 管理本机环境，**不写入游戏内存、不注入 DLL**。独立 Mod 工具只在新输出目录生成文件，不覆盖源 Mod；两个程序只约定音频编码和 JSON 清单。

运行数据保存在程序同级目录：

- `config/global_config.json`：全局配置；
- `config/accounts/`：账号元数据、DPAPI 加密 Token、Battle.net / UnifiedAuth 快照与账号设置；
- `config/stateData/data.db`：场次、历史记录和符文声纹观测数据库；
- `logs/`：运行日志，最多自动保留 16 个。

程序不会主动上传账号配置或 Token。联网范围包括 Battle.net Token 登录页面、GitHub Releases 更新接口和邪恶区域信息接口。完整的管理员权限、进程与句柄访问、注册表、全局快捷键、进程音频捕获和 v0.9.4 杀毒软件误报说明见 [安全、权限与漏洞报告政策](SECURITY.md)。请勿在公开 Issue 中提交 Token、账号目录、个人路径或包含隐私的日志；安全问题请按该政策私下报告。

### 从源码开发

```powershell
npm install
npm run dev
npm test
npm run build
npm run build:desktop
```

环境要求、项目结构和 Windows 构建细节见 [开发指南](docs/DEVELOPMENT.md)。提交修改前请阅读 [贡献指南](CONTRIBUTING.md)。

---

## English

D2RHub is a local Windows utility for managing multiple Diablo II: Resurrected accounts, separate CN/Global client profiles, and multi-client launch workflows. It uses mod audio IDs for rune-drop tracking without reading game memory.

D2RHub is maintained as one complete desktop edition. Account management, audio telemetry, statistics, overlays, and Bongo Cat are included in the same build; MSI and NSIS are installation formats, not separate feature editions.

### Current features

- Isolated CN and Global game, save, and Battle.net profiles.
- Web Token launch or Battle.net authentication with local runtime snapshots and DPAPI-encrypted tokens.
- Single, batch, and multi-select launch controls, mod arguments, window positions, and per-account game settings.
- V7 audio-signature decoding, statistics, overlays, and all account-management features in the standard build.
- Per-process WASAPI capture, all-Area and frontend detection, lifecycle deduplication, immediate SQLite persistence, and live overlay updates. Hiding the statistics overlay does not stop enabled audio tracking.
- A bundled but independently maintained `d2r-audio-mod.exe` generator for creating a minimal Mod or augmenting an existing unpacked Mod without coupling either codebase or data model. Recipe r7 adds an in-game Next/Create/Join toolbar with a lightweight Next-game confirmation strip, chat-level focused keyboard priority for room inputs, restored native single-line input styles, and toggle/Esc/close-button exits while retaining the password cache and Enter submission. Game, source-Mod, and output paths support Unicode, spaces, and other Windows-valid characters; source tables accept UTF-8, UTF-16, GBK/GB18030, and common Windows ANSI encodings, JSON resources accept common JSON5 syntax, and empty FLAC silence overrides remain silent. Existing Mods may provide only the tables they override; each missing table is filled from the matching local game data.
- Two-phase in-game room rotation: the primary shortcut creates and submits the next numbered room; after the primary enters, the follower shortcut submits all configured follower joins in parallel, without pause-menu or lobby navigation.
- Grouped overlay drops with counts, short-lived new-drop notices, a compact latest-five view, and edge-docked auto-hide.
- Focus shortcuts, run/rune/Terror Zone overlay, and Bongo Cat.

### Quick start

1. Download an MSI / NSIS installer from [Releases](https://github.com/gjy991229/D2RHub/releases).
2. Configure at least one complete game-directory and save-directory pair, then initialize an account.
3. Select an initialized monitoring account and enable audio-signature detection. On first use, choose original gameplay or an existing Mod, then enter a name for the new Mod and click the single prepare action. Names may contain ASCII letters, numbers, hyphens, and underscores.
4. D2RHub generates and validates the new Mod, preserves unrelated launch flags, and fixes the Mod-specific arguments to `-mod <name> -txt -assettestmode 1`. Restart an already-running game once for the change to take effect.

The v7 protocol uses isolated area/drop synchronization, a 127-chip Gold signature for drop IDs, redundant packets, and CRC-6. Its 10-bit ID covers Area 1–1023; the default marker level is `-30dBFS`.

### Security and local data

D2RHub uses Windows process, handle, registry, filesystem, window, and WASAPI interfaces. It does **not** write to game memory or inject DLLs. The Mod generator writes to a new output Mod and never overwrites the selected source Mod.

Runtime data is stored beside the executable in `config/` and `logs/`, including global settings, DPAPI-encrypted tokens, local Battle.net/UnifiedAuth snapshots, per-account settings, and the SQLite statistics database. D2RHub does not intentionally upload account configuration or tokens. The separate Mod tool writes only to a new output Mod and does not read D2RHub data. See the [Security, permissions, antivirus false-positive, and vulnerability-reporting policy](SECURITY.md) for the complete Windows capability and trust-boundary disclosure.

### Development

```powershell
npm install
npm run dev
npm test
npm run build
npm run build:desktop
```

See the [Development Guide](docs/DEVELOPMENT.md) for prerequisites and Windows build details. Please read [CONTRIBUTING.md](CONTRIBUTING.md) before submitting a change.

### License and trademarks

Source code and original assets that D2RHub has the right to license are available under the [MIT License](LICENSE). Third-party dependencies and marks remain under their own terms; see [Third-Party Notices](THIRD_PARTY_NOTICES.md).

D2RHub is an unofficial third-party project and is not affiliated with, authorized, sponsored, or endorsed by Blizzard Entertainment. Diablo, Diablo II: Resurrected, Battle.net, and related names are trademarks or registered trademarks of Blizzard Entertainment.

---

<div align="center">
  <a href="https://github.com/gjy991229/D2RHub/issues">Report a bug</a> ·
  <a href="https://github.com/gjy991229/D2RHub/discussions">Discussions</a>
</div>
