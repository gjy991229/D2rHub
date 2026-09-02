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
- **多账号启动控制**：单账号、启动全部、可持久化启动方案、常用方案直启、取消队列、运行状态与启动日志；启动方案按自己的成员和账号卡片当前顺序启动。
- **账号独立游戏配置**：图形化编辑显示、图形、音频、玩法和地图设置。
- **音频数据包**：按 v7 协议识别 33 个符文、50 个扩展物品、全部游戏 Area 与主界面，并按目标 D2R PID 捕获实际混音输出；可设置最低记录符文编号，且掉落仅在已确认的野外/地下城场景入库。隐藏场景统计悬浮窗不会停止已开启的声纹监控。
- **独立功能组 Mod 加工**：D2RHub 内置独立的 `d2r-audio-mod.exe` 生成器，可按需加入声纹识别、局内房间工具和显式选择的“死亡后自动退房”。死亡退房会保留原死亡界面，在死亡判定后约 0.11 秒离开当前游戏，不能避免死亡或挽救专家模式角色；能力指纹只标识支持情况，不包含启停状态。安装后可在“设置 → 可选功能 → Mod 管理”的具体 Mod 条目中切换，D2RHub 仅原子增删死亡界面的具名定时入口，不调用加工器或重建 Mod；正在使用该 Mod 的游戏需先关闭，并在下次启动时生效。无论从原版还是现有 Mod 准备，软件都会生成新 Mod、复核功能组清单并安全配置账号启动参数。游戏、源 Mod 与输出目录支持中文、空格及 Windows 允许的特殊字符；源 Mod 数据表兼容 UTF-8、UTF-16、GBK/GB18030 与常见 Windows ANSI 编码，JSON 资源兼容常见 JSON5 写法，0 字节 FLAC 静音占位也会保持静音。生成器代码仍在独立仓库，不读取 D2RHub 配置或数据库，也不修改源 Mod。
- **自动刷图统计**：每个不同野外独立计时，主城和主界面停止并结算；统计页可同时勾选并编辑女伯爵、地穴、安达利尔、墨菲斯托、Chaos、巴尔等常用 Farm 策略组，也可创建和编辑自定义路线。策略组内分段先合计耗时，再作为一场参与场次与平均耗时计算；重叠策略不会重复计数，原始数据保持不变。筛选器可折叠，离群优化默认开启，短空场阈值默认 1 秒且可调。
- **分组掉落反馈**：场景统计悬浮窗按物品分组显示重复掉落并标注数量；新掉落会短时弹出提示，列表默认保留最近 5 种，可按需展开全部，并支持贴边自动隐藏。双击空白区域或按 Enter 可切换为仅显示识别场景、计时和场次的迷你模式；迷你窗口固定位置并开启鼠标穿透。
- **可选自动跟房**：以多开核心中的已运行账号为边界，主号建房成功后可手动或延时让小号并行跟进；任务支持取消、同房间失败重试、账号租约和下一房序号持久化。启用前会验证受信任的启动快照、局内房间工具与 F13 聊天键位，停用后自动回收快捷键、watcher 和工作线程。
- **快捷键与桌宠**：按账号位置聚焦游戏窗口；Bongo Cat 支持缩放、气泡和可解锁皮肤。

### 快速开始

1. 从 [Releases](https://github.com/gjy991229/D2RHub/releases) 下载 MSI / NSIS 安装包。
2. 首次运行时至少完整配置一套“游戏目录 + 存档目录”。
3. 添加并初始化账号，然后从账号卡片启动单个账号、启动全部账号，或创建可重复使用的启动方案；在“启动方案”菜单点星标，可把常用方案固定到主界面直接启动。
4. 在 D2RHub 选择已初始化账号并开启音频声纹识别。首次开启时选择“我玩原版”或要保留功能的现有 Mod，并为生成的新 Mod 输入名称，然后点击“一键准备并开启”。名称仅可使用英文字母、数字、短横线和下划线。
5. 软件自动生成新的识别 Mod，并在保留其他无关启动参数的前提下将 Mod 参数固定为 `-mod <名称> -txt -assettestmode 1`。若游戏已经运行，重启该账号后生效。

默认 v7 协议使用相互隔离的地点/掉落同步码、127 chip 掉落 Gold 签名、双份数据包和 CRC-6；10 位 ID 可覆盖 Area 1–1023，默认声纹增益为 `-30dBFS`。D2RHub 内的实现与测试见 [音频遥测 v7](docs/AUDIO_TELEMETRY_V7.md)，生产端的正式协议文档随独立 Mod 工具仓库维护。

### 数据与安全边界

D2RHub 通过 Windows 进程、句柄、注册表、文件、窗口和 WASAPI 管理本机环境，**不写入游戏内存、不注入 DLL**。独立 Mod 工具只在新输出目录生成文件，不覆盖源 Mod；两个程序通过版本化功能组清单约定声纹与 UI 布局产物。

运行数据保存在当前 Windows 用户的 `%APPDATA%\D2RHub` 目录；日志仍保存在程序同级 `logs` 目录：

- `%APPDATA%\D2RHub\global_config.json`：全局配置；
- `%APPDATA%\D2RHub\modules\`：可选模块的版本化 sidecar 配置、备份与自动恢复数据；
- `%APPDATA%\D2RHub\accounts\`：账号元数据、DPAPI 加密 Token、Battle.net / UnifiedAuth 快照与账号设置；
- `%APPDATA%\D2RHub\stateData\data.db`：场次、历史记录和符文声纹观测数据库；
- `logs/`：运行日志，最多自动保留 16 个。

升级自便携目录格式时，程序会自动把完整的旧 `config/` 搬迁到用户数据目录并保留迁移标记；迁移失败会继续使用旧目录并在下次启动重试。若新旧两处都已有配置、账号或统计数据，程序不会静默合并或覆盖任何一边，并会在日志中报告冲突；完成手工核对前请同时备份两处目录。

程序不会主动上传账号配置或 Token。联网范围包括 Battle.net Token 登录页面、GitHub Releases 更新接口和邪恶区域信息接口。完整的管理员权限、进程与句柄访问、注册表、全局快捷键、进程音频捕获和 v0.9.4 杀毒软件误报说明见 [安全、权限与漏洞报告政策](SECURITY.md)。请勿在公开 Issue 中提交 Token、账号目录、个人路径或包含隐私的日志；安全问题请按该政策私下报告。

### 从源码开发

```powershell
npm install
npm run dev
npm test
npm run build
npm run build:desktop
```

环境要求、项目结构和 Windows 构建细节见 [开发指南](docs/DEVELOPMENT.md)。核心、平台服务、可选能力与配置兼容边界见 [架构决策 ADR 0002](docs/adr/0002-core-and-capability-module-architecture.md)。提交修改前请阅读 [贡献指南](CONTRIBUTING.md)。

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
- A bundled but independently maintained `d2r-audio-mod.exe` generator for adding independently verified audio recognition, in-game room tools, and an explicit opt-in auto-exit-after-death feature. The latter preserves the death UI and leaves the current game roughly 0.11 seconds after death is already confirmed; it cannot prevent death or save a Hardcore character. Its stable capability fingerprint is independent of activation state. The switch on the concrete Mod row atomically adds or removes only the named death-screen timer while no game instance is using that Mod; it does not invoke the generator or rebuild the Mod. Game, source-Mod, and output paths support Unicode, spaces, and other Windows-valid characters; source JSON5 and common table encodings are normalized only in the new output Mod, never in the source.
- Grouped overlay drops with counts, short-lived new-drop notices, a compact latest-five view, and edge-docked auto-hide. Double-clicking an empty area or pressing Enter switches to a fixed click-through mini view that shows only the detected scene, timer, and run count.
- Optional room automation built on trusted running instances, with staged primary/follower actions, cancellation, same-room retry, account leases, durable sequence updates, and lifecycle-owned shortcuts/F13 binding.
- Focus shortcuts, run/rune/Terror Zone overlay, and Bongo Cat.

### Quick start

1. Download an MSI / NSIS installer from [Releases](https://github.com/gjy991229/D2RHub/releases).
2. Configure at least one complete game-directory and save-directory pair, then initialize an account.
3. Select an initialized monitoring account and enable audio-signature detection. On first use, choose original gameplay or an existing Mod, then enter a name for the new Mod and click the single prepare action. Names may contain ASCII letters, numbers, hyphens, and underscores.
4. D2RHub generates and validates the new Mod, preserves unrelated launch flags, and fixes the Mod-specific arguments to `-mod <name> -txt -assettestmode 1`. Restart an already-running game once for the change to take effect.

The v7 protocol uses isolated area/drop synchronization, a 127-chip Gold signature for drop IDs, redundant packets, and CRC-6. Its 10-bit ID covers Area 1–1023; the default marker level is `-30dBFS`.

### Security and local data

D2RHub uses Windows process, handle, registry, filesystem, window, and WASAPI interfaces. It does **not** write to game memory or inject DLLs. The Mod generator writes to a new output Mod and never overwrites the selected source Mod.

Runtime data is stored in `%APPDATA%\D2RHub`; launch logs remain in `logs/` beside the executable. An existing portable `config/` directory is migrated automatically with a recoverable fallback, and conflicting populated locations are never silently merged or overwritten. If a conflict is reported, back up both locations until they have been reconciled. The data includes global settings, DPAPI-encrypted tokens, local Battle.net/UnifiedAuth snapshots, per-account settings, and the SQLite statistics database. D2RHub does not intentionally upload account configuration or tokens. The separate Mod tool writes only to a new output Mod and does not read D2RHub data. See the [Security, permissions, antivirus false-positive, and vulnerability-reporting policy](SECURITY.md) for the complete Windows capability and trust-boundary disclosure.

### Development

```powershell
npm install
npm run dev
npm test
npm run build
npm run build:desktop
```

See the [Development Guide](docs/DEVELOPMENT.md) for prerequisites and Windows build details, and [ADR 0002](docs/adr/0002-core-and-capability-module-architecture.md) for the core/platform/capability and configuration-compatibility boundaries. Please read [CONTRIBUTING.md](CONTRIBUTING.md) before submitting a change.

### License and trademarks

Source code and original assets that D2RHub has the right to license are available under the [MIT License](LICENSE). Third-party dependencies and marks remain under their own terms; see [Third-Party Notices](THIRD_PARTY_NOTICES.md).

D2RHub is an unofficial third-party project and is not affiliated with, authorized, sponsored, or endorsed by Blizzard Entertainment. Diablo, Diablo II: Resurrected, Battle.net, and related names are trademarks or registered trademarks of Blizzard Entertainment.

---

<div align="center">
  <a href="https://github.com/gjy991229/D2RHub/issues">Report a bug</a> ·
  <a href="https://github.com/gjy991229/D2RHub/discussions">Discussions</a>
</div>
