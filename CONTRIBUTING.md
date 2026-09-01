# 贡献指南

感谢你愿意改进 D2RHub。

## 开始之前

1. 搜索现有 Issues 和 Pull Requests，避免重复工作。
2. 对较大的功能或行为变化，先创建 Issue 说明使用场景和预期结果。
3. 从最新 `main` 创建聚焦的功能分支；一个 PR 只解决一个主题。

## 修改原则

- 遵循现有 React、TypeScript 和 Rust 代码风格。
- 不夹带与目标无关的重构、依赖升级或格式化。
- 不提交账号 Token、注册表转储、含个人信息的截图、本地日志或生成的构建
  产物。
- 新增联网行为、持久化数据或系统权限时，必须在 PR 中明确说明。
- 用户可见行为变化应同步更新 README 或开发文档。

## 架构边界

- 多开编排是不可关闭的核心；系统适配、配置迁移、日志和 IPC 属于平台层；其余能力通过模块注册表接入并可独立启停。
- 前端业务代码不得直接调用 `@tauri-apps/api/core` 或 `@tauri-apps/api/event`，统一通过 `src/platform/tauri` 的类型化网关访问。
- 设置项按 `src/features/settings/settingsRegistry.ts` 注册，并由独立面板承载；不要继续向设置中心协调器堆叠整块 JSX。
- 新设置模块应在模块内提供类型化的中英文文案；全局 DOM 文案观察器只用于兼容旧页面，不能作为新模块的翻译扩展点。
- 持久化格式必须向后兼容。新增或修改配置字段时，需要提供默认值、幂等迁移和旧版本样例测试；不能要求用户手工改配置。
- Windows、进程和窗口相关代码留在平台适配层，领域模型不能反向依赖具体功能模块。

## 提交前验证

在仓库根目录运行：

```powershell
npm ci
npm test
npm exec tsc -- --noEmit
npm run build
Set-Location src-tauri
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
Set-Location ..
```

PR 描述应包含：

- 解决的问题和行为变化；
- 已运行的验证命令及结果；
- 必要的脱敏截图；
- 已知限制或后续工作。

公开 CI 会在 Windows 上重新运行测试和构建，但不能代替本地验证。

## 许可

除非提交时明确说明并获得维护者同意，你提交给本项目的贡献将按项目的
[MIT License](LICENSE) 提供。不要提交你无权再分发的代码、模型、图片、字体或其他
素材。

## 协作行为

请保持尊重、耐心并聚焦技术事实。骚扰、歧视、人身攻击、泄露他人隐私或恶意干扰
协作的行为不会被接受，维护者可以关闭相关讨论或拒绝贡献。
