# Mod 资源分发与加工器独立安装

> 此文记录第一版分发方式。双源下载、原位更新和兼容升级以 [双源更新方案](dual-source-updates.md) 为准。

## 产品边界

标准 LiteHub、BoHub、NullHub 使用预生成成品。D2RHub 提供资源下载、完整性校验、安装位置管理、账号分配和加工入口；加工器独立发布，不进入 NSIS/MSI，也不再提交 EXE 到源码。加工器源码仍归属 d2r-audio-mod 仓库。

GitHub D2rHub 仓库的 Releases 附件承载 EXE 和 ZIP。资源标签使用 `mod-resources-日期.序号`，标记为 prerelease 且 `latest=false`，防止老客户端的 `/releases/latest` 把资源识别为安装包。正式 Hub 安装包继续使用 `v*` 标签。

源码仅保存小型 `resources/mod-resources-v1.json` 清单，包含资源 URL、字节数、SHA-256、版本和 Mod 数据版本。每次资源更新创建新标签，不覆盖旧附件。清单通道描述兼容的声纹协议、配方与轻量格式；破坏性升级应使用新通道和新 Hub，不直接给旧 Hub 推送不兼容加工器。

## 用户路径

1. 设置 → Mod 管理 → 下载 Mod 与加工器。沿用当前国服/国际服页签。
2. 显示资源描述、体积、推荐版本、现有加工器版本和准确安装目录。
3. 点击下载并安装，后台任务显示进度和取消入口；可关闭弹窗继续下载。
4. 加工器进入 `%LOCALAPPDATA%\com.d2rhub.app\tools\d2r-audio-mod\<版本>-<摘要前12位>\d2r-audio-mod.exe`。升级不依赖管理员权限，也不改动 Hub 安装目录。
5. 成品进入所选游戏目录的 `mods\LiteHub`、`mods\BoHub`、`mods\NullHub`；完成后刷新共享 Mod 列表，用户自行选择账号应用，不自动改变正在使用的 Mod。
6. 网络不通时可点击浏览器下载，再用导入已下载文件选择 EXE/ZIP。导入仍核验完整清单，不依赖下载文件原来的文件名。

## 版本和迁移

- 打开资源页或加工页自动检查一次远端清单，本地状态先显示；另有手动检查更新。
- 已检查的清单缓存到加工器目录，网络失败继续使用缓存或 Hub 自带清单。revision 单调递增，拒绝远端降级。
- 优先检查当前推荐加工器的大小与 SHA-256。校验失败、缺失或旧版本会阻止加工并提示下载。
- 识别已记录的旧独立加工器，以及 Hub EXE / resource_dir 旁旧安装器留下的加工器，通过带超时的 `--version` 探测版本。版本未知也明确显示。
- 旧内置文件不自动执行加工、不自动删除；用户安装兼容版本后 Hub 只调用自己的受管路径。可直接从旧 EXE 导入，但必须与已发布版本的完整校验值一致。
- 新版本放在独立目录，旧版本仍保留。切换依据当前兼容清单，下载失败不会发布新目录。
- 三个初始成品来自指定本机 mods 文件夹，游戏数据版本为 93854。安装时对照 `.build.info` 中激活行的版本末段，无法识别或不一致时拒绝自动安装；等待适配资源或用独立生成器自行生成。
- 同名 Mod 一律不覆盖，尤其不覆盖用户已加工或调整的 Mod。资源页提示使用现有成品。需要重装时先自行移走现有目录，再安装；不会隐式修改账号分配。

## 事务与校验

下载使用 HTTPS，仅接受指定 GitHub 仓库 Releases 资源地址。限制清单、下载大小、ZIP 文件数、解压总量和单文件大小。解压拒绝路径穿越、绝对路径、Windows 保留名、ADS、符号链接与不区分大小写的重复文件。

每次任务在目标目录同一磁盘创建 UUID 临时目录。先验证下载字节数和 SHA-256；加工器执行版本探测；Mod 核验生成清单、方案身份、modinfo 和游戏数据版本。全部通过再原子改名发布，失败或取消只清理本次任务目录。安装与 Mod 加工共享互斥锁，避免同时使用或更新加工器。

任务不支持任务中心一键重放：失败后在资源页重试或导入本地下载文件。网络中断不保留可执行半成品；当前实现重新下载，不做断点续传。进程被强制结束时可能留下带 `.d2rhub-resource-` 前缀的任务目录，不会作为安装成功目录使用。

## 发布维护

```powershell
cargo build --release --locked --manifest-path D:\pro\d2r-audio-mod\Cargo.toml
python scripts/package-mod-resources.py --mods 'C:\Diablo II Resurrected\mods' --processor D:\pro\d2r-audio-mod\target\release\d2r-audio-mod.exe --output artifacts/mod-resources-20260927.1 --tag mod-resources-20260927.1 --revision 2026092701
```

脚本只读来源 Mod，生成 ZIP、独立 EXE 和清单。分发清单不包含生成机器的绝对安装路径；ZIP 保留成品的资源文件与生成来源说明。先以 draft 上传所有资源并验证，之后发布 prerelease，再更新 main 上的资源清单。保存资源包的精确摘要，可重用已构建附件，不能用同一标签上传不同内容。

加工器冒烟测试通过 `D2RHUB_MOD_PROCESSOR` 指定独立 EXE，通过 `D2RHUB_LIGHTWEIGHT_GAME_ROOT` 指定游戏目录。打包校验不代表游戏内视觉或性能已完成实测。
