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
- **音频数据包**：为 33 个符文和地点生成带 CRC 的 v4 超声标记，并按目标 D2R PID 捕获实际混音输出。
- **v4.3 女伯爵实机版**：符文使用单向 `Flippy → Ground` 状态机且按编号错峰发送；地点标记覆盖罗格营地、黑色荒地、遗忘之塔与高塔地牢 1–5 层。
- **自动刷图统计**：每个不同野外独立计时，主城和主界面停止并结算；统计页可用自定义策略把同一次连续行程中的黑色荒地、高塔 1–5 层等分段合并展示，原始数据不变。
- **快捷键与桌宠**：按账号位置聚焦游戏窗口；Bongo Cat 支持缩放、气泡和可解锁皮肤。

### 快速开始

1. 从 [Releases](https://github.com/gjy991229/D2RHub/releases) 下载 MSI / NSIS 安装包。
2. 首次运行时至少完整配置一套“游戏目录 + 存档目录”。
3. 添加并初始化账号，然后从账号卡片启动单个账号或批量启动。
4. 在“设置中心 → 自动化”选择完整的 jcy.mpq，生成 `D2RHubAudioCountessV43`，启动参数使用 `-mod D2RHubAudioCountessV43 -txt`。
5. 选择已初始化账号并开启音频声纹识别；目标 D2R 启动后会按 PID 自动监听、显示诊断、计时和记录掉落。

默认 v4 协议使用 18kHz BPSK 前导、17/19kHz FSK 载荷、双份数据包和 CRC-6；10 位 ID 可覆盖 Area 1–1023，默认声纹增益为 `-30dBFS`。设计、安装和诊断见 [音频遥测 v4](docs/AUDIO_TELEMETRY_V4.md)。

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
- A v4.3 Countess field build with one-shot rune drop states, rune-specific transmission slots, and continuous-ambience IDs for the complete Countess route.
- Per-process WASAPI capture, independent wilderness segments, town/frontend stopping, presentation-only merge strategies, immediate SQLite persistence, and live overlay updates.
- Focus shortcuts, run/rune/Terror Zone overlay, and Bongo Cat.

### Quick start

1. Download an MSI / NSIS installer from [Releases](https://github.com/gjy991229/D2RHub/releases).
2. Configure at least one complete game-directory and save-directory pair, then initialize an account.
3. Under **Settings → Automation**, generate `D2RHubAudioCountessV43` from a complete unpacked source mod, then launch with `-mod D2RHubAudioCountessV43 -txt`.
4. Select an initialized monitoring account and enable audio-ID detection. Monitoring starts automatically after its D2R process launches and exposes live capture diagnostics.

The v4 protocol uses an 18kHz BPSK preamble, a 17/19kHz FSK payload, two packet copies, and CRC-6. Its 10-bit ID covers Area 1–1023; the default marker level is `-30dBFS`.

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
