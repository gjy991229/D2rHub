<div align="center">
  <img src="logo.png" alt="D2RHub Logo" width="112" />
  <h1>D2RHub</h1>
  <p><strong>Diablo II: Resurrected 多账号、多版本与音频遥测刷图助手</strong></p>
  <p>
    <img src="https://img.shields.io/badge/version-0.8.0-d5a85a" alt="Version 0.8.0" />
    <img src="https://img.shields.io/badge/platform-Windows_11-blue" alt="Windows 11" />
    <img src="https://img.shields.io/badge/game_memory-no_injection-2f855a" alt="No game-memory injection" />
    <img src="https://img.shields.io/badge/license-MIT-lightgrey" alt="MIT License" />
  </p>
</div>

---

## 中文

D2RHub 是一款 Windows 本地工具，用于管理《暗黑破坏神 II：重制版》的多个账号、国服/国际服客户端配置和多开流程，并通过 mod 音频声纹记录符文掉落和本地统计数据。

### 当前能力

- **国服 / 国际服双版本隔离**：分别配置游戏、存档和 Battle.net 路径；亚服、美服、欧服共用国际服档案。
- **两种账号认证**：网页 Token 直启，或 Battle.net 客户端认证与本地运行快照；Token 使用 Windows DPAPI 加密保存。
- **多账号启动控制**：单账号、启动全部、多选启动、取消队列、运行状态与启动日志；支持账号排序、mod 参数和窗口位置。
- **账号独立游戏配置**：图形化编辑显示、图形、音频、玩法和地图设置。
- **音频身份证**：为 33 个符文、首批 7 个区域和主界面植入 41 个低相关 Gold 扩频码，按目标 D2R PID 捕获；兼容 `r1`/`r01` 命名。
- **内置 FLAC 制作工具**：处理 `r1`–`r33`、`a1`、`a6`、`a21`–`a25` 与 `frontend.flac`，输出到独立的 `D2RHubTagged` 目录并逐文件解码自检。
- **自动刷图统计**：每个不同野外独立计时，主城和主界面停止并结算；统计页可用自定义策略把同一次连续行程中的黑色荒地、高塔 1–5 层等分段合并展示，原始数据不变。
- **快捷键与桌宠**：按账号位置聚焦游戏窗口；Bongo Cat 支持缩放、气泡和可解锁皮肤。

### 快速开始

1. 从 [Releases](https://github.com/gjy991229/D2RHub/releases) 下载 MSI / NSIS 安装包。
2. 首次运行时至少完整配置一套“游戏目录 + 存档目录”。
3. 添加并初始化账号，然后从账号卡片启动单个账号或批量启动。
4. 在“设置中心 → 自动化”处理包含 `r1.flac`–`r33.flac`、区域 `a{AreaId}.flac` 与 `frontend.flac` 的目录，再把自检通过的文件放回 mod 对应资源位置。
5. 选择已初始化账号并开启音频声纹识别；目标 D2R 启动后会按 PID 自动监听、计时和记录掉落。

默认 v3 协议使用 14kHz 载波、长度 63 的 Gold 码和三次重复。处理器支持 1–8 声道、8–32 位、采样率不低于 32kHz 的 FLAC；默认声纹增益为 `-26dBFS`。

### 数据与安全边界

D2RHub 通过 Windows 进程、句柄、注册表、文件、窗口和 WASAPI 管理本机环境，**不写入游戏内存、不注入 DLL**。FLAC 工具只在独立输出目录生成用户指定 mod 音频的处理副本，不覆盖源文件。

运行数据保存在程序同级目录：

- `config/global_config.json`：全局配置；
- `config/accounts/`：账号元数据、DPAPI 加密 Token、Battle.net / UnifiedAuth 快照与账号设置；
- `config/stateData/data.db`：场次、历史记录和符文声纹观测数据库；
- `logs/`：运行日志，最多自动保留 16 个。

程序不会主动上传账号配置或 Token。联网范围包括 Battle.net Token 登录页面、GitHub Releases 更新接口和邪恶区域信息接口。请勿在公开 Issue 中提交 Token、账号目录、个人路径或包含隐私的日志；安全问题请按 [安全政策](SECURITY.md) 私下报告。

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

### Current features

- Isolated CN and Global game, save, and Battle.net profiles.
- Web Token launch or Battle.net authentication with local runtime snapshots and DPAPI-encrypted tokens.
- Single, batch, and multi-select launch controls, mod arguments, window positions, and per-account game settings.
- A built-in FLAC processor for 33 rune IDs, seven Area-ID markers, and a frontend marker, with per-file verification and non-destructive `D2RHubTagged` output.
- Per-process WASAPI capture, independent wilderness segments, town/frontend stopping, presentation-only merge strategies, immediate SQLite persistence, and live overlay updates.
- Focus shortcuts, run/rune/Terror Zone overlay, and Bongo Cat.

### Quick start

1. Download an MSI / NSIS installer from [Releases](https://github.com/gjy991229/D2RHub/releases).
2. Configure at least one complete game-directory and save-directory pair, then initialize an account.
3. Under **Settings → Automation**, process the folder containing rune, `a{AreaId}.flac`, and `frontend.flac` assets and copy the verified output into your mod.
4. Select an initialized monitoring account and enable audio-ID detection. Monitoring starts automatically after its D2R process launches.

The v3 protocol uses a 14kHz carrier, 63-chip Gold codes, and three repetitions. The processor accepts 1–8 channel, 8–32 bit FLAC files with sample rates of at least 32kHz; the default marker level is `-26dBFS`.

### Security and local data

D2RHub uses Windows process, handle, registry, filesystem, window, and WASAPI interfaces. It does **not** write to game memory or inject DLLs. The FLAC processor writes tagged copies to a separate output folder.

Runtime data is stored beside the executable in `config/` and `logs/`, including global settings, DPAPI-encrypted tokens, local Battle.net/UnifiedAuth snapshots, per-account settings, and the SQLite statistics database. D2RHub does not intentionally upload account configuration or tokens.

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
