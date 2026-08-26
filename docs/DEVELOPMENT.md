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

# 运行 Rust 库测试
Set-Location src-tauri
cargo test --lib
Set-Location ..

# 构建桌面安装包
npm run build:desktop

# 仅构建 NSIS 安装包
npm run build:nsis
```

音频遥测构建包含 FLAC 编解码、SQLite 和 Windows WASAPI 依赖。普通前端修改通常只需
运行 `npm test` 和 `npm run build`；涉及 Rust 代码时还必须运行 `cargo test --lib`。

## 项目结构

- `src/`：React 页面、组件、状态和前端工具。
- `src-tauri/src/`：Tauri 命令、Windows 集成、启动流程、符文声纹和统计逻辑。
- `src-tauri/src/rune_audio/`：共享声纹协议、FLAC 批处理和 WASAPI 实时识别。
- `public/`：Vite 直接复制的运行时图片和 SVG。
- `docs/`：用户文档、开发文档及应用内离线页面。
- `.github/workflows/`：公开仓库的 Pull Request 验证 CI。

## 本地数据与调试文件

应用运行时可能在用户数据目录保存账号配置、加密 Token、注册表快照、日志、统计
数据库。这些内容可能包含账号或个人路径，绝不能复制进仓库或附在
公开 Issue/PR 中。提交日志或截图前必须脱敏。

不要提交：

- `.env`、私钥、Token 或其他凭据；
- `node_modules/`、`dist/`、`src-tauri/target/`；
- 本地日志、注册表导出、声纹处理清单和统计数据库；
- 安装包、可执行文件或个人发布配置。

## CI 与发布

公开仓库 CI 仅验证测试和构建，使用只读权限。项目维护者的 Release 自动化不属于
公开仓库；外部贡献者无需也不能通过公开 CI 发布 D2RHub 安装包。

## 常见问题

### Rust 第一次编译很慢

FLAC、Tauri 和 Windows API 依赖量较大，冷编译可能需要数分钟。后续编译会复用
`src-tauri/target/` 缓存。

### WebView 窗口无法打开

确认系统已安装 Microsoft Edge WebView2 Runtime，并重新运行 Tauri 开发命令。

### 符文声纹监控无法启动

确认使用 64 位 MSVC Rust 工具链和较新的 Windows 11，目标账号的 D2R 进程已经运行，
并在“设置中心 → 自动化”选择了相同账号。FLAC 批处理不依赖外部 ffmpeg。
