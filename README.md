<div align="center">
  <img src="logo.png" alt="D2RHub Logo" width="120" />
  <h1>D2RHub</h1>
  <p><strong>《暗黑破坏神 II：重制版》多账号多开管理器与 OCR 刷图助手 | Diablo II: Resurrected Multi-Account Manager & OCR Run Tracker</strong></p>
  <p>
    <img src="https://img.shields.io/badge/platform-Windows_10%2F11-blue" alt="Platform" />
    <img src="https://img.shields.io/badge/no_memory_injection-local_tool-green" alt="Local Tool" />
    <img src="https://img.shields.io/badge/license-MIT-lightgrey" alt="License" />
  </p>
</div>

---

## 🇨🇳 中文说明

D2RHub 是一款专为《暗黑破坏神 II：重制版》(D2R) 玩家精心设计的**多账号管理与一键多开及 OCR 辅助助手**。无论是多账号挂机、装备倒腾、多角色配置管理，还是刷图效率统计，它都能让您的游戏体验变得无比轻松和高效。

### ✨ 核心特色功能
*   **🎮 本地多开管理**：一键清除游戏的多开限制。基于 Windows 标准进程句柄清理技术，不修改游戏文件，不注入内存或 DLL。
*   **🚀 账号独立配置**：支持为每个账号绑定独立的战网 Token 实现自动排队登录，支持独立启动参数（Mod 启动）以及独立的单机存档目录。
*   **⚙️ 图形化画质设置**：内置画质编辑器，支持一键切换画质预设（如小号极低画质、大号超高画质），降低多开时的硬件负载。存档目录缺少 `Settings.json` 时，账号创建、登录和多开仍可正常使用，仅画质配置相关功能暂不可用。
*   **📊 硬件负载监控**：实时监控 CPU、内存、GPU 及显存占用，多开时硬件状态一目了然。
*   **🪟 迷你悬浮窗 (Overlay)**：可置顶悬浮在桌面上，支持展开与迷你两种布局。展开模式下双击空白处、迷你模式下双击悬浮窗或聚焦后按 Enter/空格即可切换；两种布局会分别记住窗口尺寸。迷你窗口压缩到 36px 高时会自动改为单行，并可继续缩小到 18px；重新拉高到 36px 时恢复双行。展开模式下双击账号可聚焦对应游戏窗口。
*   **⏱️ OCR 智能刷图统计**：通过实时 OCR 分析游戏画面，自动记录刷图时间、历史平均用时、总场次等统计数据。
*   **💎 符文掉落自动记录**：自动匹配游戏内掉落的符文并记录，提供可视化数据分析。
*   **🐱 桌面键盘猫咪 (Bongo Cat)**：可爱的桌面键盘同步互动猫咪，支持自定义皮肤和按键响应。

### 🛠️ 快速上手步骤
1.  **下载软件**：前往 [Releases 页面](https://github.com/gjy991229/D2RHub/releases) 下载最新的版本。
    *   *安装包版（推荐）*：下载 Release 中的 `.msi` 或 NSIS `.exe` 安装包，双击安装即可。
2.  **首次运行配置**：启动 D2RHub，设置向导会自动扫描并帮您填写好战网客户端路径、游戏目录以及存档路径，确认无误后点击"保存并继续"。
3.  **添加与启动账号**：
    *   点击主界面的"添加账号"，输入您的账号别名。
    *   点击"初始化"，软件会自动关联您当前的战网和游戏配置。
    *   勾选想要启动的账号，点击"一键启动"，软件就会帮您自动排队登录并清除多开限制！

### 🛡️ 安全说明
D2RHub 本质上是一个本地配置、进程与窗口管理器。它通过 Windows 系统 API 关闭 D2R.exe 的单实例检测句柄，**不修改游戏文件、不写入游戏内存、不注入 DLL**。但任何第三方工具都可能受游戏服务条款、杀毒软件误报、系统权限和账号风控策略影响；请自行评估风险并保管好账号凭据。

### 🔐 数据与隐私
D2RHub 的账号配置、Token 加密数据、注册表快照、日志、OCR 调试截图和本地统计数据库均保存在本机，不会主动上传账号配置或 Token。程序会访问 Battle.net 登录页面、GitHub Releases 更新接口和恐怖地带信息接口；启用 OCR 调试输出时，程序可能在本地保存游戏截图裁剪图用于排查识别问题。不需要调试时建议关闭该选项并定期清理本地数据。

### 🧑‍💻 从源码开发
开发环境、常用命令、项目结构和 Windows 构建说明见 [开发指南](docs/DEVELOPMENT.md)。提交修改前请阅读 [贡献指南](CONTRIBUTING.md)。

### ⚖️ 许可与第三方声明
项目有权许可的源码与原创素材采用 [MIT License](LICENSE)。PaddleOCR 模型、第三方依赖和第三方商标继续适用各自的许可或权利声明，详见 [第三方声明](THIRD_PARTY_NOTICES.md)。

D2RHub 是非官方第三方工具，与 Blizzard Entertainment 不存在隶属、授权、赞助或背书关系。Diablo、Diablo II: Resurrected、Battle.net 及相关名称是 Blizzard Entertainment 的商标或注册商标。

### 💬 反馈与交流
如果您在使用中遇到任何问题，欢迎提出：
*   [提交 Bug 或建议 (Issues)](https://github.com/gjy991229/D2RHub/issues)
*   开发者 QQ：`980102315`

请勿在公开 Issue 中提交账号 Token、含个人路径的日志或其他敏感数据。安全问题请按 [安全政策](SECURITY.md) 私下报告。

---

## 🇺🇸 English Description

D2RHub is a lightweight, player-friendly Windows utility designed for managing multiple accounts, enabling multi-boxing (multi-clienting), and OCR-based run tracking in **Diablo II: Resurrected (D2R)**.

### ✨ Features
*   **🎮 Local Multi-Instance Management**: Bypass the D2R single-instance restriction automatically. D2RHub uses native Windows handle closing APIs, with no memory injection, DLL injection, or game-file modification.
*   **🚀 Configuration Isolation**: Bind different Battle.net accounts, custom launch arguments (Mod support), and **independent save directories** for each profile.
*   **⚙️ Graphical Settings Editor**: Tweak D2R's `Settings.json` with a full GUI. Apply graphics presets instantly (e.g., Ultra settings for your main character, Minimum settings for alt accounts to save GPU/CPU resources). If the save directory does not contain `Settings.json`, profile creation, login, and multiboxing remain available; only graphics configuration features are unavailable until the path or file is fixed.
*   **📊 Hardware Load Monitor**: Real-time tracking of CPU, RAM, GPU, and Video Memory (VRAM) so you can monitor your computer's health while multi-boxing.
*   **🪟 Desktop Overlay**: A semi-transparent always-on-top overlay supports expanded and mini layouts. Double-click the expanded background, double-click anywhere in mini mode, or focus the overlay and press Enter/Space to switch modes. Each mode remembers its own size. The mini layout reflows to one row at 36px high, can shrink to 18px, and returns to two rows when raised back to 36px. Double-clicking a profile in expanded mode focuses its game window.
*   **⏱️ OCR Run Tracker**: Real-time OCR-based game screen analysis. Automatically records run timers, average times, and session run counts.
*   **💎 Rune Drop Logging**: Automatically recognizes and logs high-value rune drops with visual stats.
*   **🐱 Interactive Bongo Cat**: A cute animated desktop pet that mimics your keyboard strokes! It triggers unique visual animations when high-value loot (Uniques/Runes) drops in the game.

### 🛠️ Quick Start
1.  **Download**: Get the latest MSI or NSIS installer from the [Releases](https://github.com/gjy991229/D2RHub/releases) page.
2.  **First Run**: The wizard will auto-detect your Battle.net, D2R game path, and save folders. Click **Save & Continue**.
3.  **Manage Profiles**: Click **Add Account**, name it, click **Initialize**, select the accounts you wish to launch, and click **Launch**. D2RHub handles the login queue and clears the mutexes automatically.

### Security and Data
D2RHub stores configuration, encrypted token data, registry snapshots, logs, OCR debug images, and local stats databases on your own machine. It does not intentionally upload account configuration or tokens. The app connects to Battle.net login pages, the GitHub Releases API, and a Terror Zone information API. Use of any third-party utility can still be affected by game terms, antivirus false positives, system permissions, and account risk controls.

Do not post account tokens, personal filesystem paths, or other sensitive data in public Issues. Report vulnerabilities privately according to the [Security Policy](SECURITY.md).

### Development and Contributions
See the [Development Guide](docs/DEVELOPMENT.md) for prerequisites, commands, and project structure. Contributions are welcome; please read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request.

### License and Trademarks
Source code and original assets that D2RHub has the right to license are available under the [MIT License](LICENSE). OCR models, third-party dependencies, and trademarks remain subject to their own terms; see [Third-Party Notices](THIRD_PARTY_NOTICES.md).

D2RHub is an unofficial third-party project and is not affiliated with, authorized, sponsored, or endorsed by Blizzard Entertainment. Diablo, Diablo II: Resurrected, Battle.net, and related names are trademarks or registered trademarks of Blizzard Entertainment.

---

<div align="center">
  <p>
    <a href="https://github.com/gjy991229/D2RHub/issues">Report Bug</a> ·
    <a href="https://github.com/gjy991229/D2RHub/discussions">Discussions</a>
  </p>
</div>
