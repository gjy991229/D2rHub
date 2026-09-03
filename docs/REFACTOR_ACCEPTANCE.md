# 模块化单体重构验收记录

本记录是重构分支的发布门禁。D2rHub 仍是编译期组合、统一发布的 Windows
模块化单体，不提供第三方 SDK，不动态加载 DLL，也不拆分微服务。

## 自动验收

| 阶段 | 已建立的可验证边界 |
| --- | --- |
| 1 基线 | 前端、Rust、Release 构建和无副作用启动冒烟；真实游戏验收单独列为人工门禁 |
| 2 后端分层 | `domain`、`application`、`infrastructure`、`commands` 分层；系统命令只做 IPC 转换，Windows 实现位于基础设施 |
| 3 核心模型 | 账号、实例、启动方案、取消代次、账号/目录/宿主租约由多开核心统一管理 |
| 4 能力协议 | 静态能力清单声明 ID、版本、分类、依赖、配置版本、设置入口、命令、事件、生命周期和健康状态 |
| 5 配置兼容 | v0-v9 样本迁移、未知字段保留、CAS、staging/backup、失败恢复和跨资源 journal 测试 |
| 6 前端架构 | Tauri 原始 API 仅存在于平台网关；设置面板和控制器按功能拆分；壳组件设有 900 行回归上限 |
| 7 设置中心 | 导航、搜索、可用性和能力状态由注册表组合；简单/复杂设置均由功能面板拥有 |
| 8 任务运行时 | 初始化、启动、Mod 加工和自动跟房共享任务状态、冲突键、取消、时间线、错误码和后端重试 |
| 9 诊断 | 结构化任务时间线、能力健康和脱敏日志可导出 ZIP；测试验证路径、账号和凭据不会泄漏 |
| 10 资源治理 | 可选窗口按需创建并在停用时销毁；worker、监听器和快捷键由能力生命周期回收；提供 Release 基线脚本 |
| 11 测试体系 | Rust 单元/迁移/集成测试、前端逻辑/交互/同步测试、架构契约、严格 Clippy 和生产构建 |
| 12 发布收口 | 每个垂直阶段独立提交；正式命令 `npm run tauri build` 生成 EXE、MSI 和 NSIS 包 |

每次 RC 必须依次执行：

```powershell
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
npm run tauri build
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure-release-baseline.ps1
```

## 人工实机门禁

下列行为不能由仓库测试或无副作用冒烟替代，必须在真实 D2R、Battle.net、账号和
本机 F13 环境中完成。它们不是重构遗留代码，而是 RC 发布前的外部系统验收：

- CN/Global、Token/Battle.net、多账号串行启动、停止和窗口切换；
- 自动跟房的主号、跟随号、取消、失败重试和 F13 聊天绑定；
- 音频遥测、识别 Mod 安装/升级、掉落与场景统计；
- 存档目录监听、覆盖层定位、多显示器恢复和全局快捷键；
- 从真实旧版数据升级、失败恢复备份，以及安装包覆盖升级和回滚。

人工门禁失败时不得发布；修复必须增加能够稳定复现该缺陷的自动测试，然后重新执行
全部自动验收。
