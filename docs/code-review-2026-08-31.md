# D2RHub 代码审查报告（2026-08-31）

## 结论

本次“取消待机 / Dock、保留启动方案、增加常用方案直启”的实现没有发现阻断发布的问题。新增链路的职责边界较清楚：启动方案可用性与常用 ID 处理集中在纯函数中，主界面只负责组合，Rust 配置层负责最终归一化。

按“框架清晰、逻辑明确、符合直觉、设计优雅”四个维度评估当前代码库：

| 维度 | 评分 | 结论 |
| --- | ---: | --- |
| 框架清晰 | 6.5 / 10 | 前后端边界明确，但多个超大文件承担过多职责。 |
| 逻辑明确 | 8 / 10 | 核心校验偏保守、失败闭合，配置迁移测试较扎实。 |
| 符合直觉 | 8.5 / 10 | 主路径、危险操作和不可用态大多能从界面直接理解。 |
| 设计优雅 | 7 / 10 | 纯工具函数和原子配置保存较好；全量配置写入、运行时 DOM 翻译和逐卡 IPC 拉低了整体一致性。 |

综合：**7.5 / 10**。当前适合继续迭代，但下一阶段应优先解决配置并发写与前端职责收敛，而不是继续向 `App.tsx`、`AccountCard.tsx`、`SettingsCenter.tsx` 追加功能。

## 主要发现

### P1 · 全量配置保存存在陈旧快照覆盖风险

位置：[`src/store/globalConfig.ts`](../src/store/globalConfig.ts#L37)、[`src/App.tsx`](../src/App.tsx#L259)、[`src/components/config/SettingsCenter.tsx`](../src/components/config/SettingsCenter.tsx#L657)

`save(config)` 每次把完整 `GlobalConfig` 发送给后端。多个窗口或组件若基于不同时间点的 `config` 同时保存，Rust 的 I/O 锁只能保证写文件不并行，不能阻止后到的陈旧快照覆盖先到的新字段。常用方案、主题、桌宠和设置中心都走这条路径。

建议：新增字段级 `patch_global_config`，或在 store 内串行化函数式更新 `update(current => next)`；后端增加 revision / compare-and-swap，发生版本冲突时明确重试或提示。

### P2 · 账号卡片承担数据访问、编辑事务、拖曳和展示四类职责

位置：[`src/components/dashboard/AccountCard.tsx`](../src/components/dashboard/AccountCard.tsx#L163)

该文件约 1100 行，同时处理 Tauri IPC、事件监听、Mod 增删、位置配置、画质防抖保存、方案编辑、账号状态和卡片 UI。修改任何一条路径时都需要理解整张卡的生命周期，容易产生输入事件与拖曳事件互相影响——本次空格触发拖曳就是这种耦合的典型表现。

建议拆为：`useAccountQuickSettings`、`useAccountModEditor`、`AccountCardHeader`、`AccountCardActions`、`AccountQuickSettingsDrawer`；`SortableAccountCard` 只保留拖曳适配层。

### P2 · 折叠状态下仍为每个已初始化账号读取设置并注册监听

位置：[`src/components/dashboard/AccountCard.tsx`](../src/components/dashboard/AccountCard.tsx#L222)、[`src/components/dashboard/AccountCard.tsx`](../src/components/dashboard/AccountCard.tsx#L256)

每张已初始化卡片挂载后都会调用一次 `get_account_settings`，并各自注册 `account-settings-updated` 监听。账号数量增加时，主页初始化会产生 O(N) 次 IPC 与 O(N) 个监听器，即使抽屉从未展开。

建议：普通模式首次展开时再加载；方案编辑需要画质数据时使用批量命令一次读取，或由账号 store 做按账号缓存并只注册一个全局监听。

### P2 · 国际化依赖 DOM 后处理，文案与业务数据边界脆弱

位置：[`src/i18n.tsx`](../src/i18n.tsx#L6)、[`src/i18n.tsx`](../src/i18n.tsx#L895)、[`src/i18n.tsx`](../src/i18n.tsx#L998)

当前以中文原文为 key，再用 `MutationObserver` 遍历并替换文本和属性。文案微调会静默漏翻；短语替换可能命中账号名、方案名等用户数据；频繁 DOM 更新还会重复遍历子树。

建议逐步改为稳定 key 的 `t("launch.favorite.add")`，动态值使用参数插值；迁移期间保留现有 runtime 兜底，但新组件不再新增 DOM 后处理词条。

### P2 · 页面编排和后端命令文件已经超过单文件可理解边界

位置：[`src/App.tsx`](../src/App.tsx#L98)、[`src/components/config/SettingsCenter.tsx`](../src/components/config/SettingsCenter.tsx#L306)、[`src-tauri/src/commands/global_config.rs`](../src-tauri/src/commands/global_config.rs#L2046)

`App.tsx` 约 787 行，`SettingsCenter.tsx` 约 2948 行；Rust 的 `global_config.rs`、`account.rs`、`launch.rs` 均接近或超过 3000 行。文件名已经无法准确表达内部所有职责，审查和回归范围随功能增长而扩大。

建议按业务域切分，而不是按“组件/命令”继续堆叠：前端拆 `useLaunchGroupController`、`MainActionBar`、设置页各 tab；Rust 拆 `config/schema.rs`、`config/migration.rs`、`config/validation.rs`、`config/repository.rs`，账号命令按认证、配置、生命周期拆分。

### P2 · 自动测试缺少 React 交互与拖曳回归层

位置：[`scripts/run-tests.cjs`](../scripts/run-tests.cjs#L5)

现有前端测试偏重纯函数，且通过 TypeScript 转译后直接执行；`npm run build` 能补足类型检查，但菜单门户、键盘拖曳、输入框事件传播等交互只能靠人工视觉审查。因此本次“Mod 参数输入空格”无法由现有测试套件自动防回归。

建议引入 Vitest + Testing Library 覆盖组件键盘事件，再用 Playwright 保留 3 条主流程：Mod 参数含空格、启动方案星标固定/移除、不可用常用方案禁止启动。

### P3 · 前后端配置结构由人工同步

位置：[`src/store/types.ts`](../src/store/types.ts#L16)、[`src-tauri/src/commands/global_config.rs`](../src-tauri/src/commands/global_config.rs#L69)

`GlobalConfig` 在 TypeScript 与 Rust 各维护一份。新增字段必须同时修改默认值、视觉审查数据和两端类型，遗漏时通常要到运行期才暴露。

建议从 Rust schema 生成 TypeScript 类型，或至少增加一个契约测试：序列化 Rust 默认配置后由前端 schema 校验。

## 做得好的部分

- 配置文件采用 staging + backup 的原子替换策略，读取失败时倾向失败闭合，避免静默覆盖用户配置。
- 启动方案在启动前完整检查账号、Token、Mod 与位置引用，避免“部分账号悄悄启动”的模糊状态。
- 账号排序、方案物化、常用方案归一化都放在可独立测试的纯函数中。
- 危险的“一键关闭”有二次确认；不可用方案有明确警告态和原因提示。
- 菜单使用 portal、ARIA 和键盘关闭/焦点恢复；本次常用方案仍沿用同一套交互语法，没有引入新的二级导航。

## 建议实施顺序

1. 先解决全量配置并发覆盖：它是唯一可能导致用户设置丢失的结构性风险。
2. 抽出账号卡片的数据 hooks，并把设置读取改成懒加载或批量加载。
3. 为本次三个关键交互补自动化浏览器测试。
4. 拆分 `App.tsx` / `SettingsCenter.tsx` 与 Rust 大型命令文件。
5. 渐进替换 DOM 后处理国际化，并建立前后端配置契约。

## 本次验收证据

- `npm test`：全部通过。
- `npm run build`：TypeScript 与 Vite 生产构建通过。
- `cargo fmt --check`：通过。
- `cargo test --manifest-path src-tauri/Cargo.toml commands::global_config`：52 项通过。
- `cargo test --manifest-path src-tauri/Cargo.toml`：244 项通过，3 项依赖外部音频/提权环境的测试按预期忽略。
- 浏览器视觉验收：操作栏顺序为“启动全部 → 启动方案 → 常用方案 → 一键关闭”；菜单星标可以固定方案；不可用方案保持禁用警告态。
- 浏览器逐键输入 `-mod alpha beta`：空格正常保留，焦点仍在输入框，卡片未进入拖曳态。
