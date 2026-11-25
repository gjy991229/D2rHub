# 资源与产物归属

本目录保存分发协议需要的小型清单。加工器随 Hub 安装包发布；Mod 成品独立下载，准备与发布统一使用 [发布工作流](../docs/release-workflow.md)。

| 内容 | 来源与维护者 | Git / 打包边界 |
| --- | --- | --- |
| `mod-resources-v2.json` | 本项目的资源目录与兼容声明 | 跟踪；新客户端的内置基线，远端更新经校验后使用 |
| `mod-resources-v1.json` | 旧客户端的历史资源目录 | 跟踪并冻结语义；不由新工作流改写 |
| `scene-shortcuts-v6.json`、`scene-shortcuts-v7-actinfo.json` | 快捷场景配方的布局摘要与小站映射基线 | 跟踪；Rust 校验器通过 `include_str!` 编译嵌入，不能作为临时输出清理 |
| `vision-resources-v1.json` | 视觉识别模型与运行库的版本、下载地址和摘要 | 跟踪；编译嵌入作为资源下载的固定回退清单，模型和 DLL 本体不入库 |
| `jcy-inventory-v1.json` | JCY 背包与仓库预设的大小、摘要和双端下载地址 | 跟踪；加工需要时下载到应用数据目录的 `resources/jcy-inventory`，预设本体不进入源码或安装包 |
| `area-name-aliases.json` | 从用户提供的瓜哥计时器 5.29 `levels-ch.json`、`levels-chtw.json`、`levels-mdk.json` 提取 zhCN / zhTW / zhTWO 地图名称及入场提示译名，保留源文件 SHA-256 | 跟踪；由 `scripts/generate-area-name-aliases.py <原 resources 目录>` 重建，Rust 编译嵌入；按原始 Key 关联，去掉颜色码、任务横幅与数字等级标注，不推测近似译名 |
| `public/logo.png`、根 `logo.png` | 项目作者提供的同一 Logo | 跟踪；`public/` 进入前端构建，根文件保留兼容引用 |
| `public/bongo-cat-*.svg` | 项目作者原创文件，形象灵感另有声明 | 跟踪；运行时资源 |
| `public/token-copy-guide.png`、赞助图片与 JSON | 项目作者提供 | 跟踪；运行时资源 |
| `src-tauri/icons/` | 应用图标的各尺寸产物 | 跟踪；安装器与应用资源 |
| `docs/user-guide.html`、`docs/stats.html` | 本项目维护的离线页面 | 跟踪；由 Tauri 资源清单显式打包 |
| `docs/guide-images/` | 真实组件、模拟状态的手册图示 | 跟踪；只打包手册实际引用的文件 |
| `docs/pet-visual-review/` 等审查图 | 有日期的开发验证证据 | 跟踪；不进入安装包 |
| `dev/previews/` | 演示夹具与历史设计稿 | 跟踪；只在 Vite 开发时使用 |
| `src-tauri/gen/schemas/` | Tauri 生成的 IPC / 权限描述 | 当前跟踪；随 API 改动核对，不手工维护 |
| `dist/`、`target/`、`artifacts/` | 编译、打包与发布任务 | 忽略；不进入源码提交 |
| Mod ZIP、含加工器的 Hub 安装器 | 独立源码构建或已确认成品目录 | Release 附件；只在验证后对外提供 |

素材授权见 [第三方声明](../THIRD_PARTY_NOTICES.md)。这份表记录出处，不替代上游许可证。加工器的源码、协议、许可与构建说明由 [d2r-audio-mod](https://github.com/gjy991229/d2r-audio-mod) 维护。

安装器通过 `src-tauri/tauri.conf.json` 显式声明离线文档资源；`npm run check:guide` 验证路由、图片引用、当前版本和打包清单。新增图片时更新来源说明与打包声明；不要把整个 docs 或开发预览目录复制到安装器。

分发清单保存兼容范围、版本、大小和 SHA-256；本机绝对路径、账号数据、发布凭据与生成目录不属于清单。维护工具在 Git 外保存源路径与发布任务；任务记录保留精确来源提交与字节摘要，便于复查和补传。

图标尺寸文件由主图生成后提交。需要更新图标时使用项目已安装的 Tauri CLI：`npm run tauri icon -- public/logo.png`，复核各尺寸透明度与外观后再提交。旧 `create-icon.js/.cjs` 是早期占位脚本，会将 ICO 字节写成 PNG，已移除；不要用其历史版本重建正式图标。
