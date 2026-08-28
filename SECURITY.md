# 安全、权限与漏洞报告政策

## 开源与信任边界

D2RHub 是开源的 Windows 本地工具，源代码、构建配置和依赖声明均保存在本仓库。
请只从本仓库的 [Releases](https://github.com/gjy991229/D2RHub/releases) 下载发布产物，
并在安装前核对 Release 页面公布的文件名和校验值。

开源便于审查，但不等于杀毒软件白名单，也不能仅凭仓库内容证明任意来源的二进制文件
由该源码构建。Windows 安全产品仍会根据文件签名、下载信誉、压缩方式和运行行为独立
判断风险。

## Windows 权限与系统能力

D2RHub 当前主程序请求管理员权限。下表列出程序使用的主要权限、用途和安全边界。

| 权限或系统能力 | 用途 | 安全边界 |
| --- | --- | --- |
| 管理员权限 | 查询并关闭 D2R 的多开限制句柄、管理 Battle.net / Agent 进程，以及启动 ETW 监听等需要较高权限的 Windows 操作。 | 不用于关闭 Windows Defender、防火墙或其他安全软件。 |
| 进程查询、启动与终止 | 检测、启动或关闭指定的 `Battle.net.exe`、`Agent.exe` 和 `D2R.exe`；管理批量启动队列和账号对应的游戏 PID。 | 进程操作按名称、PID、安装路径或账号上下文进行约束。 |
| 系统句柄访问 | 枚举目标 D2R 进程的 Event 句柄，并关闭 `DiabloII Check For Other Instances` 多开限制句柄。 | 不向游戏注入 DLL，不调用游戏内存读写接口。 |
| 注册表读写 | 备份、清理和恢复 `HKCU\Software\Blizzard Entertainment\Battle.net\UnifiedAuth`，并写入启动所需的区域、语言和 `WEB_TOKEN` 数据。 | 认证快照只用于本机账号切换；Token 使用 Windows DPAPI 保护，不通过前端 IPC 返回明文。 |
| ETW 系统事件监听 | 短时监听 D2R 是否成功读取 `WEB_TOKEN`，用于确认当前账号已完成认证消费，再继续多账号启动队列。 | 监听按注册表值名和目标游戏 PID 匹配，不用于收集其他应用的注册表内容。 |
| 文件系统读写 | 保存全局配置、账号运行快照、游戏 `Settings.json`、日志、统计数据库和音频识别数据；准备新的识别 Mod。 | 数据保存在本机；Mod 工具写入用户选择的新输出目录，不覆盖源 Mod。 |
| 全局键盘和鼠标钩子 | 匹配用户配置的账号窗口聚焦快捷键，并为 Bongo Cat 提供键盘、左右键动画事件。 | 会检查虚拟键和修饰键组合，但不保存按键文本、密码或输入历史，也不通过网络发送输入事件。 |
| 进程音频捕获 | 使用 Windows WASAPI application loopback 捕获指定 D2R PID 的实际混音输出，用于地点和掉落声纹识别。 | 不访问麦克风；捕获范围限定为所选 D2R 应用及其进程树。 |
| 窗口管理 | 查找 D2R 窗口、修改窗口标题和位置、聚焦指定账号窗口，并显示置顶悬浮窗。 | 只操作 D2RHub 自身窗口及已识别的游戏窗口。 |
| 网络访问与外部浏览器 | 打开 Battle.net 官方登录页、查询 GitHub Releases 更新、获取邪恶区域公开信息。统计页 API 绑定 Windows 回环地址。 | 本地统计服务仅监听 `127.0.0.1`；程序不设计用于上传账号配置、Token 或 UnifiedAuth 快照。 |

## 本地数据和联网范围

默认运行数据保存在程序同级目录：

- `config/global_config.json`：全局设置；
- `config/accounts/`：账号元数据、DPAPI 加密 Token、Battle.net / UnifiedAuth 快照和账号设置；
- `config/stateData/data.db`：场次、历史记录和声纹观测数据库；
- `logs/`：运行日志，最多自动保留 16 个。

当前源码中的主动联网范围包括：

- Battle.net 官方登录页面；
- GitHub Releases 最新版本接口；
- `api.d2-trade.com.cn` 的邪恶区域公开信息接口；
- 绑定随机端口的 `127.0.0.1` 本地统计服务。

程序不设计用于上传 Battle.net Token、UnifiedAuth 快照、账号目录、键鼠输入或本地配置
文件。独立的 `d2r-audio-mod.exe` 生成器不读取 D2RHub 配置或统计数据库。

## 不会执行的操作

D2RHub 当前实现不会：

- 向 D2R 或 Battle.net 进程注入 DLL；
- 读取、写入或扫描游戏内存；
- 修改 `D2R.exe` 或其他游戏可执行文件；
- 录制麦克风；
- 保存或上传用户输入的按键内容；
- 关闭或修改 Windows Defender、防火墙及其他安全软件；
- 在用户不知情的情况下安装附加软件。

## 关于 v0.9.4 的 VirusTotal 检测

2026-08-28 上传 VirusTotal 的 NSIS 安装包信息如下：

- 文件：`D2RHub_0.9.4_x64-setup.exe`
- SHA-256：`3184b219008334cfda7f73d76a024f91c0a8134060cb202ac1d5ddd31f623625`
- 检测结果：4 / 71 个引擎报告风险，其余引擎未检出；结果可能随引擎更新或重新分析而变化。
- 该版本安装器及其主要可执行文件未使用 Authenticode 代码签名。

报告风险的名称包括 `Trojan-PSW...Bobik.gen`、`Wacatac.B!ml`、泛化
`W32.Malware` 和 `Unsafe`。其中 `gen`、`generic` 和 `!ml` 通常表示泛化、启发式
或机器学习判断；VirusTotal 的热门威胁标签由厂商结果聚合而来，不等于已经确认文件
属于某个具体木马家族。

该结果更可能由以下特征组合触发：

- 新发布且没有代码签名，缺少文件哈希和发布者信誉；
- NSIS 安装器对内部可执行文件进行压缩；
- 主程序请求管理员权限；
- 程序需要管理 Battle.net 认证注册表和 `WEB_TOKEN`；
- 程序会终止指定进程、枚举并关闭目标游戏进程句柄；
- 程序使用 ETW 和全局低级键盘、鼠标钩子。

这些能力是账号切换、多开、快捷键和桌宠功能所需，但也可能被密码窃取程序、木马或
外挂滥用，因此容易触发少数安全产品的行为模型。4 / 71 既不能单独证明文件有恶意，
也不能作为绝对安全证明；用户仍应核对下载来源和校验值。

VirusTotal 只汇总各安全厂商的结果，误报需要由产生检测的厂商复核。维护者会对最终
发布产物分别检查安装器、主程序和随附工具，向命中厂商提交误报，并逐步完善统一代码
签名、可验证构建、校验值和最小权限设计。项目不会要求用户通过关闭杀毒软件来解决
误报。

参考：

- [VirusTotal：误报处理说明](https://docs.virustotal.com/docs/false-positive)
- [Microsoft：提交文件进行恶意软件分析](https://www.microsoft.com/en-us/wdsi/filesubmission)
- [Microsoft：Windows 应用的 SmartScreen 信誉](https://learn.microsoft.com/zh-cn/windows/apps/package-and-deploy/smartscreen-reputation)

## 支持范围

安全修复以最新 Release 和公开仓库当前 `main` 为目标。较旧版本可能只收到升级
建议，不保证单独回补。

## 私下报告漏洞

请优先使用仓库 Security 页面中的 **Report a vulnerability**（GitHub Private
Vulnerability Reporting）提交报告。若该入口暂未启用，请先通过 README 中的
开发者 QQ 联系维护者索取私密报告渠道，不要在公开 Issue 中披露漏洞细节。

报告请包含：

- 受影响版本和 Windows 版本；
- 影响范围及攻击前提；
- 可复现的最小步骤或概念验证；
- 建议的缓解方法；
- 是否已向其他人披露。

请使用专门创建的测试数据。不要发送真实 Battle.net 账号、Token、注册表凭据、
个人文件路径或未脱敏截图。

维护者会尽快确认报告、评估影响并协调修复和披露时间，但不承诺固定响应时限或漏洞
奖励。

## 不属于安全漏洞的事项

以下内容通常应在脱敏后提交普通 Issue：

- 安装、启动或声纹识别问题；
- 杀毒软件误报；
- 游戏服务条款解释、封号风险咨询或账号申诉；
- 不涉及安全边界的崩溃和功能建议。
