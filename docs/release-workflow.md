# 一键发布工作流

本页是当前唯一的维护者发布入口；资源协议细节见 [双源更新协议](dual-source-updates.md)。

日常使用仓库根目录的 `release.cmd`（双击），选择 **软件（含加工器） / 三个Mod / 全部**。脚本自动准备文件并显示版本、兼容范围、大小；输入 `p` 上传双端，输入 `s` 上传并提升软件正式版，直接回车则仅保留准备结果。选择“补传已有任务”可恢复失败的发布，不重新构建。

## 首次配置

需要 Windows、Python 3.10+、PowerShell、GitHub CLI。构建软件还需要项目开发环境（Node.js、Rust、Windows 构建工具），构建加工器需要 Rust。先运行：

```powershell
python -m pip install -r scripts/publisher-requirements.txt
gh auth login
```

现有本机的 Gitee 凭据继续使用 `%LOCALAPPDATA%\D2RHub-Publisher\config.json` 及 DPAPI 加密令牌。换电脑需要重新配置凭据，不能复制另一 Windows 用户的加密令牌使用。令牌不填写在命令行、源码或发布配置中。

首次运行自动生成 `%LOCALAPPDATA%\D2RHub-Publisher\workflow.json`。菜单“修改本机路径配置”可以调整：

| 字段 | 用途 / 默认值 |
| --- | --- |
| `processor_repo` | 主仓库相邻的 `d2r-audio-mod` 源码仓库（支持 Git worktree） |
| `mods_root` | `C:\Diablo II Resurrected\mods` |
| `output_root` | 首次运行所在仓库的 `artifacts\releases` |
| `publisher_config` | 已有发布凭据配置路径 |

相对路径以配置文件目录为基准。路径配置与凭据在 Git 外，`artifacts` 也不提交。可以用 `-Config` 指定另一份本机路径配置。

## 命令行：用户与代理使用同一入口

在仓库根目录运行；无需手动复制安装包、填写 spec 或计算摘要。

```powershell
# 交互菜单，也可双击 release.cmd
.\release.ps1

# 只准备，不上传
.\release.ps1 -Target mods

# 自动准备并发布指定类别
.\release.ps1 -Target mods -Publish
.\release.ps1 -Target software -Publish
.\release.ps1 -Target all -Publish

# 正式发布软件，同时维护旧客户端依赖的 GitHub latest
.\release.ps1 -Target software -Publish -Promote

# 网络失败后复用原始文件补传；路径取自上一轮输出
.\release.ps1 -Resume "D:\release-jobs\20260928090000-example" -Publish
```

`npm run release -- --target mods --publish` 是对应的 Python 命令行入口。`--resume` 与 `--target` 互斥；`--promote` 必须同时使用 `--publish`，且任务必须包含软件。

## 文件从哪里来

- **软件**：从运行脚本所在的干净 Git 提交准备。核对快照内 npm、Cargo、Tauri 版本一致，运行前端和 Rust 检查，再执行锁定依赖的 NSIS 构建，并核对 EXE 内嵌版本。不会误取历史 `target` 中的旧安装包。
- **加工器**：从配置的干净 Git 仓库准备。先运行 Rust 格式、Clippy 和测试，再执行 `cargo build --locked --release`，检查程序实际 `--version` 与快照内 Cargo 版本一致。
- **三个 Mod**：从配置目录读取 `LiteHub`、`BoHub`、`NullHub`。支持旧 `generation-manifest.json` 与当前成品的 `enhancement-manifest.json`；校验来源、方案、实际游戏数据版本。当前成品还核对 `mod-version.json`、Mod 名称、完整文件列表及每个文件的大小和 SHA-256；拒绝加工清单、符号链接和重解析点。只去除 Hub 安装记录与可由同目录 TXT 生成的 BIN 缓存，规范化旧生成清单中的本机路径。ZIP 使用固定时间和属性，相同内容得到相同文件。

软件和加工器先通过 `git archive` 导出当前提交的源码快照，再在本次任务独立的源码及构建目录运行构建。未跟踪的历史权限文件、旧构建缓存和本机文件不会混入源码，构建不会改写原工作区；原始 Mod 也不会被修改。完成后得到一个时间戳任务目录，包含安装包/ZIP、自动生成的 `software.json` / `resources.json`、每个 Mod 的文件摘要、记录源码提交与文件摘要的 `job.json`。准备未完成时不会生成可发布任务。

正式软件发布页使用 `D2RHub v版本号` 标题。准备阶段从源码快照的 `docs/releases/v版本号.md` 读取发行说明，固定上海时区发布日期与来源提交，随任务摘要保存；缺少或空白说明会在构建前报错。提升正式版时同步写入 GitHub 和 Gitee，补传复用准备时的说明，不读取后来改动的工作区文案。旧发布任务保留现有正文。

**版本仍由开发者维护**：软件与加工器读取源码版本，不擅自递增；同版本重新构建得到不同内容时，发布器会拒绝。Mod 版本自动以 UTC 时间生成；与当前双端资源摘要相同的文件保留原版本，不重复上传。首次迁移旧打包格式时 ZIP 字节可能不同，会形成一次新资源版本。

资源兼容范围与协议通道读取仓库的 `resources/mod-resources-v2.json`。协议发生破坏性变更时，应先审查并更新此兼容声明，工作流不会猜测兼容性或自动放宽范围。

加工器只随 Hub 安装包发布。软件准备会先从加工器源码快照完成检查和构建，将同一 EXE 放入 Hub 快照的 `src-tauri/processor/`，再检查和构建 Hub。两份源码及检查记录均写入任务。独立 `processor` 发布目标已移除，资源目录只包含三个 Mod。开发构建通过 `npm run build:processor` 准备资源，可用 `D2RHUB_PROCESSOR_REPO` 指定源码位置。

发布前同步两个源码仓库，并将 CI 的加工器 checkout `ref` 固定到本次加工器提交，避免以后重跑 CI 时读取变化的 main。Hub 版本同步维护 `package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock` 和 `src-tauri/tauri.conf.json`；版本说明写入 `docs/releases/` 并链接更新记录。已准备的资源任务不修改 spec 或摘要，使用原任务 `-Resume ... -Publish` 发布。

不再绑定 Hub 和加工器软件版本。更新模块时同步修改对应协议和结构验证，不能让旧布局冒用新模块版本。

**生成记录不能代替游戏内验收。** 当前成品会重新核对 `enhancement-manifest.json` 中的文件摘要；旧生成格式仍只能证明生成时的声明。工作流会拦截可识别的加工记录并保存准确摘要，保留原有 `runtime_verified` 状态；不会把 `verified_output_integrity` 当作运行行为的证明。

## 发布前检查与源码对应

新任务在归档出来的源码快照中执行检查，任何检查失败都会停止准备，不生成可发布的 `job.json`：

- Hub：`npm ci`、`npm run check`、Rust 格式检查、带 `--locked` 的严格 Clippy（所有 targets/features）和 Rust 测试，然后构建 NSIS。
- 加工器：Rust 格式检查、带 `--locked` 的严格 Clippy（所有 targets/features）和 Rust 测试，然后构建 release EXE。

`npm run check` 汇总 TypeScript、前端测试、手册和发布器检查，需要先安装 Python 发布器依赖。任务目录中的 `verification-hub.json` / `verification-processor.json` 记录源码提交、源码快照摘要、各条命令、执行目录、时间和退出结果；schema 2 的 `job.json` 同时固定证据文件摘要。补传会核对这些证据与来源一致，不补做或伪造原任务的检查结果。

自动检查不运行被忽略的真实游戏用例，也不能代替登录、音频、跟房和升级验收。发布前仍需按 [实机验收记录](REFACTOR_ACCEPTANCE.md) 验证受影响流程。

Hub 安装器的 `source_commit` 来自用于 `git archive` 的完整提交 SHA，并与 `job.json` 的 `hub_commit` 交叉核对。上传前该提交必须已经存在于 GitHub；软件标签使用此提交创建，已有标签若指向其他提交则在上传任何一端前拒绝发布，不再默认指向当时的 main。GitHub 忽略已存在标签的 `target_commitish`，因此工作流还解析实际标签提交。[GitHub Release API](https://docs.github.com/en/rest/releases/releases#create-a-release)

Gitee 仓库只镜像附件，不镜像 Hub 源码；两端软件清单均记录同一个 GitHub 来源 SHA。资源/索引标签仍属于各自资源仓库。旧 schema 1 任务可从已有 `job.json` 读取 `hub_commit` 后补传，保留其原始字节与既有校验，不给旧任务追加新的检查声明或重新构建；缺少来源、来源不一致或已有错误软件标签时必须先解决记录问题，工具不会改写已发布标签。

## 发布、重试与正式版

底层仍调用 `publish-downloads.py`：同一份字节分别上传 Release 附件，匿名下载验证后才更新清单。重跑已有任务先核对全部文件及 spec 的摘要；任何修改均阻止上传。每次补传使用更高 revision 和独立的 `attempts` 报告，不会重建文件或覆盖同版本不同内容。

退出码：`0` 成功（或仅准备成功）；`1` 失败；`2` 已有平台可用，但镜像未完成。镜像失败时用同一任务补传，不要重新运行构建命令代替补传。

**`-Publish` 会更新新客户端实际读取的清单，已经是对外发布。** 对应软件 Release 默认保留 prerelease，旧客户端的 `/releases/latest` 不变化。`-Promote` 在双端完整验证后，将该软件 Release 提升为正式版；拒绝把 GitHub latest 降到更低版本。资源和索引永远不提升为软件 latest。跨平台操作不是原子事务，中断后仍需补传/重试。

构建缓存和发布任务不会自动删除，便于复核和补传；磁盘空间不足时由发布者清理不再需要的旧任务。尚未接入“合并 main 自动构建发布”，需要主动运行入口。

`npm run package:mod-resources` 保留为只准备 Mod 的兼容别名，等价于 `npm run release -- --target mods`。旧 v1 打包脚本已停止写文件；旧清单与旧附件仍保留兼容。

发布前需要完成 [贡献指南](../CONTRIBUTING.md) 中对应实机验收。准备成功说明源码检查与产物校验通过；上传及提升正式版仍由维护者显式选择。

验证：`npm run test:publisher`。测试覆盖确定性打包、加工目录拦截、数据版本不符、检查失败阻止构建、证据及任务篡改、失败补传、未变化资源跳过、源码与标签一致性、旧任务兼容，以及超过 100 个历史快照时的附件分页。测试使用隔离目录和模拟发布适配器，不上传实际文件。
