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

# 构建桌面安装包（正式 RC 命令）
npm run tauri build

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

辅助 WebView 在 `tauri.conf.json` 中保留完整窗口规格，但使用 `create: false`，避免未启用模块在
启动时创建 renderer。启动阶段只为已启用功能创建对应窗口；运行期间第一次启用时通过统一窗口
工厂创建，停用时销毁 renderer 和原生窗口，再次启用时按静态规格重建。窗口位置保存在独立的
版本化文件中。窗口创建、位置恢复和 capability worker 的生命周期
必须保持独立，不能由前端绕过原生可见性服务直接创建窗口。

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

自动跟房策略 v18 在模块 sidecar 中新增 `input_method`（`background_keys` /
`foreground_mouse`），缺省仍为原后台方案，沿用 generation/CAS 保存与幂等迁移。
策略 v20 撤回 v19 的后台消息点击试验。旧 `followers_background_clicks` 字段由
Serde 忽略，策略版本迁移触发 sidecar 的 CAS 重写并移除该字段，不回退版本号。
策略 v21 新增模块专属 `foreground_timing`。通过结构体默认值兼容缺省及部分旧配置，
并在现有 normalize/CAS 流程中幂等迁移。v22 将其简化为整项操作预算，旧的响应子字段
通过 Serde 忽略并随版本迁移移除；新增 `click_ms`、`paste_ms`、`difficulty_ms`
分别缺省为 50、100、50ms。切前台与开表单缺省为 100ms，按键及鼠标按住为 20ms，
操作间隔为 1ms。`finish_step` 使用 `Instant` 扣除已执行耗时，只等待剩余预算，
随后才加步骤间隔；内部按住时间计入总预算，不重复相加。粘贴预算分为全选和直接
粘贴两段，各占一半，仅空密码使用删除替代空内容粘贴。选地狱后不重新聚焦。
回车按住后抬起即返回，不调用 `finish_step`，没有提交响应等待或末尾间隔。
v23 为同一 `foreground_timing` 增加小号专属 `join_form_response_ms = 150`、
`join_focus_response_ms = 100`、`join_select_response_ms = 100`、
`join_paste_response_ms = 100`、`join_key_hold_ms = 50`。旧 sidecar 缺省补齐并经
版本迁移持久化。创建和加入仍由同一适配入口调度，但加入的 `fill_join_fields`
采用独立响应等待：开表单及焦点响应从鼠标抬起后计时，文本响应从按键抬起后计时，
不调用主号的剩余总预算算法。加入使用 NameInput 的原生 Tab 导航切到 PasswordInput。
小号密码缓存比较时序快照，修改输入参数后会重新填写；主号密码和地狱缓存保持原逻辑。
v24 移除上述主号预算 / 小号响应的分支：所有账号调用同一个 `fill_fields`，通过
同一个 `click`、`paste`、`finish_step` 实现响应等待，且全部以 Tab 切到密码框。
统一字段为 `form_response_ms`、`focus_response_ms`、`select_response_ms`、
`paste_response_ms`、`chord_hold_ms` 和 `submit_hold_ms`，分别通过 Serde alias
继承 v23 的 `join_*` 参数及原 `key_hold_ms`；旧主号预算字段被忽略并随 CAS 迁移
移除。早于 v23 的 `form_response_ms` 只是旧预算的附加响应，迁移时使用完整响应默认
150ms，避免错误继承 20ms 的片段值。界面只展示一套参数，所有账号的缓存均比较同一
时序快照。创建与加入仅在布局入口、控件坐标和首次选择地狱这项业务步骤上有差别。
鼠标按住期间和组合键按下期间的等待均可取消，返回取消前先释放已按下的输入。
此路径不读取 `FlowStrategy` 的时序值。取消信号、目标窗口与坐标校验、资源竞争等待及用户尚未
松开触发快捷键的等待继续保留，调度层的自动跟随延时和进房间隔不变。
前台方案由独立 capability 适配器编排；实际 Win32 输入、剪贴板和 DPI 作用域由
`infrastructure/physical_input.rs` 承载。它只读取已加工 Mod 的布局，不调用加工器。
所有输入归属现有可取消工作线程；缓存仅驻内存并区分进程创建时间、HWND 和表单类型，
失败前失效，避免半次填写被当成已完成。注入事件带专用标记，绕过 Hub 全局快捷键路由，
避免物理 Ctrl+A / Ctrl+V 被账号或房间快捷键吞掉。

## 本地数据与调试文件

后台跟房改为同步预填：创建快捷键先为主号及本轮可用小号并发呼出创建 / 加入表单，
主号保持前台，通过物理 Ctrl+A / Ctrl+V 配合后台窗口按键消息整段填写。
填写期间复用桌面输入互斥、快捷键暂停、按键释放和剪贴板恢复机制。
任一参与进程缺少当前密码缓存时整组重新填写密码，否则只更新房名。
主号先提交；小号保留预填表单，手动跟随或自动延时结束后只投递 Enter，仍遵循
同时 / 间隔派发设置。预填记录绑定进程创建时间、窗口和房名，未预填或重启的
小号拒绝直接提交；手动等待期间再次创建会先关闭记录中的旧表单再重新准备。
跟随阶段不会自动补填。后台逐字符间隔配置保留兼容，但不再用于新粘贴流程，
设置页隐藏该输入项。前台鼠标流程保持原行为。
物理 Ctrl 与后台消息协作仍依赖游戏实际输入处理，没有文本读取或粘贴确认；
本次变更未执行测试或游戏内验证。

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
