# D2RHub 开发指南

本文档面向希望在本地构建、测试或贡献 D2RHub 的开发者。D2RHub 目前只支持
Windows 桌面环境。

## 环境要求

- Windows 10 或 Windows 11（64 位）
- Node.js 20 或更高版本，以及随 Node.js 安装的 npm
- Rust stable 的 `x86_64-pc-windows-msvc` 工具链
- Visual Studio Build Tools，包含“使用 C++ 的桌面开发”和 Windows SDK
- Microsoft Edge WebView2 Runtime
- Git

Tauri 的 Windows 前置条件以
[Tauri 官方文档](https://v2.tauri.app/start/prerequisites/#windows)为准。

## 获取源码与安装依赖

```powershell
git clone https://github.com/gjy991229/D2RHub.git
Set-Location D2RHub
npm ci
```

`npm ci` 会严格按照 `package-lock.json` 安装前端与 Tauri CLI 依赖。Rust 依赖在
首次运行 Cargo 命令时按照 `src-tauri/Cargo.lock` 下载并编译。

## 常用命令

```powershell
# 启动 Vite 前端开发服务器
npm run dev

# 运行前端快捷键规范化测试
npm test

# TypeScript 检查并生成前端生产构建
npm run build

# 运行 Rust 库测试与严格静态检查
Set-Location src-tauri
cargo test --lib --all-features
cargo clippy --all-targets --all-features -- -D warnings
Set-Location ..

# 从相邻的独立仓库构建并更新随安装包分发的生成器 sidecar
npm run sync:audio-mod

# 构建桌面安装包
npm run build:desktop

# 仅构建 NSIS 安装包
npm run build:nsis
```

桌面程序始终包含 SQLite、WAV 诊断录音和 Windows WASAPI 声纹识别依赖。普通前端修改通常只需
运行 `npm test` 和 `npm run build`；涉及 Rust 代码时还必须运行完整 Rust 测试和严格 Clippy。

`npm test` 不只运行组件测试，也会执行架构边界检查。不要通过放宽这些规则来绕过模块依赖错误；
如果边界确实需要变化，应先更新架构决策和对应测试。

## 架构分层

D2RHub 是模块化单体，不在提权进程中加载第三方 DLL 或脚本。产品和代码都遵循同一分层：

1. **多开核心（始终启用）**：账号身份、启动上下文、账号/主机租约、实例注册、启动与退出。
2. **平台服务（必需）**：配置事务、文件恢复、Windows 适配、IPC、日志和生命周期监督。
3. **可选能力（可开关）**：桌宠、悬浮窗、统计、声纹以及自动跟房等独立功能。
4. **控制界面**：主界面操作多开核心；设置中心组合各核心区域和模块面板。

依赖只能由界面指向应用/领域，由基础设施实现应用层定义的端口。可选能力只能使用公开核心端口，
不能直接访问其他模块的命令实现或私有状态。完整约束及取舍见
[ADR 0002](adr/0002-core-and-capability-module-architecture.md)。

## 项目结构

- `src/features/`：按功能组织的 React 面板、类型化文案、验证和前端用例。
- `src/platform/tauri/`：前端唯一的 Tauri command/event 网关与契约。
- `src/components/`：跨功能复用的界面组件和仍在渐进迁移的旧组件。
- `src-tauri/src/domain/`：不依赖 Tauri 或 Windows 的稳定领域模型与规则。
- `src-tauri/src/application/`：多开核心、配置事务、能力注册表与应用用例。
- `src-tauri/src/infrastructure/`：文件事务、模块配置和其他平台适配。
- `src-tauri/src/capabilities/`：静态注册、可独立启停的第一方能力模块。
- `src-tauri/src/commands/`：薄 Tauri IPC 适配；不应承载新的业务状态机。
- `src-tauri/src/rune_audio/`：v7 协议解码、WASAPI 实时识别和掉落生命周期跟踪。
- `src-tauri/binaries/`：随安装包分发的独立生成器编译产物；其源码不在本仓库。
- `public/`：Vite 直接复制的运行时图片和 SVG。
- `docs/`：用户文档、开发文档及应用内离线页面。
- `.github/workflows/`：公开仓库的 Pull Request 验证 CI。

### 新增可选能力

一个新能力至少应同时提供：稳定 ID、类型化配置与 schema 迁移、幂等 `start`/`stop`、健康状态、
自己拥有并可回收的 worker/listener/window、薄 IPC 命令、前端 gateway，以及注册式设置面板。
停用后不得遗留线程、快捷键、窗口或机器资源。

模块专属配置写入 `%APPDATA%\D2RHub\modules\<module-id>\config.json`。必须使用共享
`ModuleConfigStore` 的 generation/CAS、staging、backup 和自动恢复能力；不要把新模块字段塞回
全局 v9 envelope。迁移旧字段时应只导入一次、保留旧值供降级使用，并保证重复启动幂等。

## 本地数据与调试文件

应用运行时可能在用户数据目录保存账号配置、加密 Token、注册表快照、日志、统计
数据库。这些内容可能包含账号或个人路径，绝不能复制进仓库或附在
公开 Issue/PR 中。提交日志或截图前必须脱敏。

不要提交：

- `.env`、私钥、Token 或其他凭据；
- `node_modules/`、`dist/`、`src-tauri/target/`；
- 本地日志、注册表导出、声纹处理清单和统计数据库；
- 安装包、个人发布配置或其他临时可执行文件。`src-tauri/binaries/` 中由
  `npm run sync:audio-mod` 更新的固定 sidecar 是发布所需的例外。

## CI 与发布

公开仓库 CI 仅验证测试和构建，使用只读权限。项目维护者的 Release 自动化不属于
公开仓库；外部贡献者无需也不能通过公开 CI 发布 D2RHub 安装包。

## 常见问题

### Rust 第一次编译很慢

Tauri、SQLite 和 Windows API 依赖量较大，冷编译可能需要数分钟。后续编译会复用
`src-tauri/target/` 缓存。

### WebView 窗口无法打开

确认系统已安装 Microsoft Edge WebView2 Runtime，并重新运行 Tauri 开发命令。

### 符文声纹监控无法启动

确认使用 64 位 MSVC Rust 工具链和较新的 Windows 11，目标账号的 D2R 进程已经运行，
并在“设置中心 → 自动化”选择了相同账号。首次开启时按界面提示一键准备识别 Mod；
若游戏已经运行，需要重启该账号一次。
