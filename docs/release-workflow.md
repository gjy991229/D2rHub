# 一键发布工作流

日常使用仓库根目录的 `release.cmd`（双击），选择 **软件 / 加工器 / 三个官方 Mod / 全部**。脚本自动准备文件并显示版本、兼容范围、大小；输入 `p` 上传双端，输入 `s` 上传并提升软件正式版，直接回车则仅保留准备结果。选择“补传已有任务”可恢复失败的发布，不重新构建。

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
.\release.ps1 -Target processor -Publish
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

- **软件**：从运行脚本所在的干净 Git 提交构建。核对 npm、Cargo、Tauri 版本一致，执行 `npm ci` 和 NSIS 构建，再核对 EXE 内嵌版本。不会误取历史 `target` 中的旧安装包。
- **加工器**：从配置的干净 Git 仓库执行 `cargo build --locked --release`，检查程序实际 `--version` 与 Cargo 版本一致。
- **三个 Mod**：从配置目录读取 `LiteHub`、`BoHub`、`NullHub`。校验生成来源、方案、实际游戏数据版本；拒绝加工清单、符号链接和重解析点。只去除 Hub 安装记录与可由同目录 TXT 生成的 BIN 缓存，规范化生成清单中的本机路径。ZIP 使用固定时间和属性，相同内容得到相同文件。

构建使用本次任务独立的目录；不会修改原始 Mod。完成后得到一个时间戳任务目录，包含安装包/EXE/ZIP、自动生成的 `software.json` / `resources.json`、每个 Mod 的文件摘要、记录源码提交与文件摘要的 `job.json`。准备未完成时不会生成可发布任务。

**版本仍由开发者维护**：软件与加工器读取源码版本，不擅自递增；同版本重新构建得到不同内容时，发布器会拒绝。Mod 版本自动以 UTC 时间生成；与当前双端资源摘要相同的文件保留原版本，不重复上传。首次迁移旧打包格式时 ZIP 字节可能不同，会形成一次新资源版本。

资源兼容范围与协议通道读取仓库的 `resources/mod-resources-v2.json`。协议发生破坏性变更时，应先审查并更新此兼容声明，工作流不会猜测兼容性或自动放宽范围。

**生成记录只能证明生成时的声明，不能证明目录后来没有手工修改。** 工作流会拦截可识别的加工记录并保存准确摘要，但发布者仍需选择经过确认的官方成品；不会把 `verified_output_integrity` 当作实时的纯净性证明。

## 发布、重试与正式版

底层仍调用 `publish-downloads.py`：同一份字节分别上传 Release 附件，匿名下载验证后才更新清单。重跑已有任务先核对全部文件及 spec 的摘要；任何修改均阻止上传。每次补传使用更高 revision 和独立的 `attempts` 报告，不会重建文件或覆盖同版本不同内容。

退出码：`0` 成功（或仅准备成功）；`1` 失败；`2` 已有平台可用，但镜像未完成。镜像失败时用同一任务补传，不要重新运行构建命令代替补传。

**`-Publish` 会更新新客户端实际读取的清单，已经是对外发布。** 对应软件 Release 默认保留 prerelease，旧客户端的 `/releases/latest` 不变化。`-Promote` 在双端完整验证后，将该软件 Release 提升为正式版；拒绝把 GitHub latest 降到更低版本。资源和索引永远不提升为软件 latest。跨平台操作不是原子事务，中断后仍需补传/重试。

构建缓存和发布任务不会自动删除，便于复核和补传；磁盘空间不足时由发布者清理不再需要的旧任务。尚未接入“合并 main 自动构建发布”，需要主动运行入口。

验证：`npm run test:publisher`。测试覆盖确定性打包、加工目录拦截、数据版本不符、任务篡改、失败补传、未变化资源跳过，以及正式发布参数检查。
