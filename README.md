<div align="center">
  <img src="public/logo.png" alt="D2RHub" width="96" />
  <h1>D2RHub</h1>
  <p>暗黑破坏神 II：重制版的 Windows 多账号工具</p>
  <p>管理账号、启动游戏，按需添加识别统计和自动跟房。</p>
</div>

当前版本 **v0.9.121**。

本版修正尸体拾取按钮覆盖头像的尺寸与偏移。完整变更见 [v0.9.121 发行说明](docs/releases/v0.9.121.md)。

[下载安装](https://github.com/gjy991229/D2RHub/releases/latest) · [使用手册](docs/user-guide.html) 提供多开、识别、跟房和组合使用的分步路线。[加工器源码](https://github.com/gjy991229/d2r-audio-mod) 单独维护，EXE 随 Hub 安装和更新，不再独立下载或互认软件版本。LiteHub、BoHub、NullHub 仍按需下载。

已有成品可增补兼容模块；覆盖更新和模块协议不一致时，从原始源 Mod 与旧模块配置生成新目录，校验后替换同名成品。批量操作提供计划预览、串行处理、逐项结果及失败重试，不自动切换账号。房间工具 r33 保留默认地狱和原创建时序，加入直接发送原生 JoinGame 消息。

## 本地数据与边界

账号、加密 Token、设置和统计存储在 `%APPDATA%\D2RHub`，日志位于程序旁的 `logs` 目录。Token 使用 Windows DPAPI 加密；程序不主动上传账号凭据，不读取或写入游戏内存，也不注入 DLL。

D2RHub 会使用进程、窗口、注册表、全局快捷键和按需启用的音频捕获能力。权限、联网范围、升级迁移及私密报告方式见 [安全政策](SECURITY.md)。公开反馈前请隐藏账号、Token、个人路径与日志中的隐私信息。

## 开发与贡献

需要 Windows、Node.js、Rust MSVC、Visual Studio C++ 构建工具和 WebView2。完整前置条件见 [开发指南](docs/DEVELOPMENT.md)。

```powershell
npm ci
npm run tauri dev
```

```powershell
python -m pip install -r scripts/publisher-requirements.txt
npm run check
npm run build
```

- [架构说明](docs/architecture.md)：产品边界、代码分层与配置归属。
- [贡献指南](CONTRIBUTING.md)：修改原则、检查命令和提交流程。
- [发布指南](docs/release-workflow.md)：准备安装器、加工器和 Mod，校验后发布与补传。

源码、运行资源和构建产物分别维护。安装包、生成的 Mod、个人配置和发布凭据不提交到 Git；公开 CI 只做验证，发布由维护者显式运行本地工具。

## English

D2RHub is a local Windows tool for managing multiple Diablo II: Resurrected accounts, separate CN/Global installations, authentication, launch groups, and per-account settings. Optional extensions provide audio-based run tracking, room automation, desktop overlays, and Bongo Cat.

Version **0.9.121** shares operational preferences across launch schemes while retaining explicit account overrides, improves localized scene recognition and timing, updates chat collaboration signals, and adds a default Hell option for in-game room creation.

[Download](https://github.com/gjy991229/D2RHub/releases/latest), configure a game and save directory, then add and initialize an account. The [Mod processor](https://github.com/gjy991229/d2r-audio-mod) ships with Hub. Account data stays on this computer; the app does not read/write game memory or inject DLLs. See the [manual](docs/user-guide.html), [development guide](docs/DEVELOPMENT.md), and [security policy](SECURITY.md).

## 许可

项目有权授权的源码与原创素材采用 [MIT License](LICENSE)，第三方素材与商标见 [第三方声明](THIRD_PARTY_NOTICES.md)。D2RHub 是非官方第三方项目，与 Blizzard Entertainment 无隶属、授权或背书关系。

[报告问题](https://github.com/gjy991229/D2RHub/issues) · [参与讨论](https://github.com/gjy991229/D2RHub/discussions)
