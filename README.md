<div align="center">
  <img src="logo.png" alt="D2RHub Logo" width="112" />
  <h1>D2RHub</h1>
  <p><strong>Diablo II: Resurrected 多账号、多版本与 OCR 刷图助手</strong></p>
  <p>
    <img src="https://img.shields.io/badge/version-0.8.0-d5a85a" alt="Version 0.8.0" />
    <img src="https://img.shields.io/badge/platform-Windows_10%2F11-blue" alt="Windows 10/11" />
    <img src="https://img.shields.io/badge/game_memory-no_injection-2f855a" alt="No game-memory injection" />
    <img src="https://img.shields.io/badge/license-MIT-lightgrey" alt="MIT License" />
  </p>
</div>

---

## 中文

D2RHub 是一款 Windows 本地工具，用于管理《暗黑破坏神 II：重制版》的多个账号、国服/国际服客户端配置和多开流程。Full 版还包含屏幕 OCR、场景计时、符文掉落记录与本地统计分析。

### 当前版本能力

- **国服 / 国际服双版本隔离**：分别配置游戏、存档和 Battle.net 路径；亚服、美服、欧服共用国际服档案，Token 账号可直接点击卡片的区服胶囊切换下次启动服务器。
- **两种账号认证**：网页 Token 直启，或 Battle.net 客户端认证与本地运行快照；Token 使用 Windows DPAPI 加密保存。
- **多账号启动控制**：单账号、启动全部、多选启动、取消队列、运行状态与启动日志；支持账号排序、Mod 参数和窗口位置。
- **账号独立游戏配置**：图形化编辑显示、图形、音频、玩法和地图设置，可在系统配置与账号快照之间切换。
- **性能悬浮窗**：展开 / 迷你布局分别记忆尺寸；显示账号状态、OCR 场景计时、符文和邪恶区域信息。拖到四边后自动吸附隐藏，悬停平滑抽出、移开再次隐藏。
- **高 DPI 迷你布局**：压缩到 40 逻辑像素时只切换为单行布局，窗口高度保持不变；单行最低 20 像素，重新拉高到 40 像素以上时恢复双行。
- **OCR 与本地统计（Full）**：识别场景和 1–33 号符文，#24 以上保存截图；提供统一筛选、效率分位数、趋势、场景/角色对比、星期×小时热力图、33 号符文图谱、高符命中率与间隔、截图画廊、记录管理和 CSV / JSON 导出。
- **快捷键与桌宠**：按账号位置聚焦游戏窗口；Bongo Cat 响应键鼠输入，支持缩放、气泡和可解锁皮肤。

### Full 与 Lite

| 功能 | Full | Lite |
| --- | :---: | :---: |
| 账号管理、双版本配置、多开 | ✓ | ✓ |
| 网页 Token / Battle.net 认证 | ✓ | ✓ |
| 画质编辑、悬浮窗、快捷键、桌宠 | ✓ | ✓ |
| 场景/符文 OCR、截图与统计页 | ✓ | — |

### 快速开始

1. 从 [Releases](https://github.com/gjy991229/D2RHub/releases) 下载 Full 或 Lite 的 MSI / NSIS 安装包。
2. 首次运行时至少配置一套包含 `D2R.exe` 的游戏目录。存档目录是账号独立画质设置所需的可选项；Battle.net.exe 只在战网客户端认证时必需。游戏和战网路径需要手动确认，存档、Agent、Roaming 和 Edge/Chrome 支持自动探测。
3. 点击“添加账号”，按向导设置昵称、区服、界面/配音语言和认证模式。网页 Token 按指引获取并粘贴；战网认证按提示完成客户端登录与初始化。
4. 从账号卡片启动单个账号，或使用“启动全部 / 多选启动”。Full 版可在“设置中心 → 自动化”选择已初始化账号并开启 OCR。

完整操作、统计口径、数据位置和排障方法见 [D2RHub v0.8 使用手册](docs/user-guide.html)。

### 数据与安全边界

D2RHub 通过 Windows 进程、句柄、注册表、文件和窗口 API 管理本机环境，**不修改游戏文件、不写入游戏内存、不注入 DLL**。任何第三方工具仍可能受到游戏服务条款、反作弊策略、安全软件、系统权限和账号风控影响，请自行评估风险。

运行数据固定保存在 `%APPDATA%\D2RHub`。从旧版本升级时，如果该系统目录尚不存在而程序同级存在 `config`，D2RHub 会在首次启动时把整个旧目录搬迁到这里；一旦系统目录存在，后续始终以系统目录为准。

- `%APPDATA%\D2RHub\global_config.json`：全局配置；
- `%APPDATA%\D2RHub\accounts\`：账号元数据、DPAPI 加密 Token、Battle.net / UnifiedAuth 快照与账号设置；
- `%APPDATA%\D2RHub\stateData\data.db`：OCR 场景和掉落数据库；
- `%APPDATA%\D2RHub\stateData\img\`：高级符文截图；
- `%APPDATA%\D2RHub\test\`：启用 OCR 调试后产生的图片和识别日志；
- `logs/`：运行日志，最多自动保留 16 个。

程序不会主动上传账号配置或 Token。联网范围包括 Battle.net Token 登录页面、GitHub Releases 更新接口和邪恶区域信息接口。请勿在公开 Issue 中提交 Token、账号目录、个人路径、包含隐私的日志或截图；安全问题请按 [安全政策](SECURITY.md) 私下报告。

“设置中心 → 维护 → 账号迁移”生成的 JSON 是跨设备迁移文件：Token 会在导出设备上解密并以**明文**写入，导入后再使用目标设备的 Windows DPAPI 加密。任何获得该文件的人都可以使用其中的 Token 登录对应账号；不要发送给他人，只保存到可信位置，并在确认导入成功后立即安全删除。

### 从源码开发

```powershell
npm install
npm run dev
npm test
npm run build
npm run build:full
npm run build:lite
```

环境要求、项目结构和 Windows 构建细节见 [开发指南](docs/DEVELOPMENT.md)。提交修改前请阅读 [贡献指南](CONTRIBUTING.md)。

---

## English

D2RHub is a local Windows utility for managing multiple Diablo II: Resurrected accounts, separate CN/Global client profiles, and multi-client launch workflows. The Full edition also includes screen OCR, run timing, rune-drop logging, and local analytics.

### Current features

- **CN / Global profile isolation** with separate game, save, and Battle.net paths; KR, NA, and EU accounts share the Global installation profile, and Token accounts can switch their next-launch server from the region badge on each card.
- **Two authentication modes**: direct Web Token launch, or Battle.net client authentication with a local runtime snapshot. Tokens are encrypted with Windows DPAPI.
- **Multi-account launch controls**: launch one, launch all, multi-select, cancel a queue, inspect progress, reorder profiles, configure mod arguments, and assign window positions.
- **Per-account game settings** for display, graphics, audio, gameplay, and automap, with system-settings and account-snapshot modes.
- **Always-on-top performance overlay** with expanded and mini layouts, account/OCR/Terror Zone status, independent size persistence, and four-edge auto-hide docking. Hover to reveal it smoothly and move away to hide it again.
- **High-DPI mini layout**: at 40 logical pixels the overlay switches to one row without changing height; the single-row minimum is 20 pixels, and heights above 40 pixels restore two rows.
- **OCR and local analytics (Full)**: scene and rune recognition, screenshots for runes #24+, global filters, timing percentiles, trends, scene/character comparisons, weekday-hour heatmaps, a 33-rune matrix, high-rune hit rate and intervals, screenshot gallery, record management, and filtered CSV/JSON export.
- **Focus shortcuts and Bongo Cat**, including input reactions, scaling, chatter, and unlockable skins.

### Full vs. Lite

| Capability | Full | Lite |
| --- | :---: | :---: |
| Account management, dual client profiles, multi-client launch | ✓ | ✓ |
| Web Token and Battle.net authentication | ✓ | ✓ |
| Game settings, overlay, shortcuts, desktop pet | ✓ | ✓ |
| Scene/rune OCR, screenshots, and analytics | ✓ | — |

### Quick start

1. Download a Full or Lite MSI / NSIS installer from [Releases](https://github.com/gjy991229/D2RHub/releases).
2. On first run, configure at least one game directory containing `D2R.exe`. A save directory is optional and only required for per-account graphics settings. Battle.net.exe is only required for Battle.net authentication. Game and Battle.net paths must be confirmed manually; save, Agent, Roaming, and Edge/Chrome paths can be detected.
3. Click **Add Account** and follow the nickname, region, UI/voice language, and authentication steps. Acquire and paste a Web Token, or complete Battle.net login and initialization.
4. Launch a profile from its card, or use **Launch All / Multi-select**. In the Full edition, select an initialized OCR target under **Settings → Automation**.

See the [v0.8 User Manual](docs/user-guide.html) for complete workflows, metric definitions, local data paths, and troubleshooting.

### Security and local data

D2RHub uses Windows process, handle, registry, filesystem, and window APIs. It does **not** modify game files, write to game memory, or inject DLLs. Any third-party utility may still be affected by game terms, anti-cheat policy, antivirus software, system permissions, and account risk controls; evaluate those risks yourself.

Runtime configuration is stored under `%APPDATA%\D2RHub`; executable-side `config` data from older releases is migrated there when the system directory does not yet exist. This includes global settings, DPAPI-encrypted tokens, local Battle.net/UnifiedAuth snapshots, per-account game settings, OCR debug images, high-rune screenshots, and the SQLite stats database. Logs remain under `logs/` beside the application. D2RHub does not intentionally upload account configuration or tokens. Network access is used for Battle.net Token login, GitHub Releases update checks, and Terror Zone information.

Account-transfer JSON files are intentionally portable: D2RHub decrypts Tokens on the source device and writes them as **plaintext**, then re-encrypts them with Windows DPAPI on the destination device during import. Anyone who obtains such a file may be able to sign in to the exported accounts. Never share it, store it only in a trusted location, and securely delete it after a successful import.

Never post tokens, account directories, personal paths, or sensitive logs/screenshots in a public Issue. Report vulnerabilities privately through the [Security Policy](SECURITY.md).

### Development

```powershell
npm install
npm run dev
npm test
npm run build
npm run build:full
npm run build:lite
```

See the [Development Guide](docs/DEVELOPMENT.md) for prerequisites, project structure, and Windows build details. Please read [CONTRIBUTING.md](CONTRIBUTING.md) before submitting a change.

### License and trademarks

Source code and original assets that D2RHub has the right to license are available under the [MIT License](LICENSE). OCR models, dependencies, and third-party marks remain under their own terms; see [Third-Party Notices](THIRD_PARTY_NOTICES.md).

D2RHub is an unofficial third-party project and is not affiliated with, authorized, sponsored, or endorsed by Blizzard Entertainment. Diablo, Diablo II: Resurrected, Battle.net, and related names are trademarks or registered trademarks of Blizzard Entertainment.

---

<div align="center">
  <a href="https://github.com/gjy991229/D2RHub/issues">Report a bug</a> ·
  <a href="https://github.com/gjy991229/D2RHub/discussions">Discussions</a>
</div>
