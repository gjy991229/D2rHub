<div align="center">
  <img src="logo.png" alt="D2Hub Logo" width="120" />
  <h1>D2Hub</h1>
  <p><strong>Diablo II: Resurrected Multi-Account Manager</strong></p>
  <p>
    <img src="https://img.shields.io/badge/version-0.1.0-blue" alt="Version" />
    <img src="https://img.shields.io/badge/tauri-v2.0-purple" alt="Tauri" />
    <img src="https://img.shields.io/badge/platform-windows-lightgrey" alt="Platform" />
  </p>
</div>

---

## English

D2Hub is a Windows desktop GUI tool for managing multiple Diablo II: Resurrected accounts. It automates the multi-boxing workflow — launching game instances, managing account configurations, applying graphics presets, and handling registry, symlinks, and process handles.

### Features

#### 🎮 Account Management
- **Account CRUD** — Create, rename, delete, and reorder accounts via drag-and-drop
- **Account Initialization** — Capture account snapshot (config, registry, saves) with one click
- **Per-Account Mod Args** — Set custom `-mod` arguments for each account independently

#### 🚀 Launch Engine
- **Batch Launch** — Launch multiple accounts in sequence with a single click
- **Battle.net Only** — Launch Battle.net without the game
- **Config Isolation** — Per-account config copy, symlinks, and registry export/import
- **Mutex Clear** — Clear process handles to allow multi-instance
- **Connection Detection** — Real-time game connection status
- **Cancel Support** — Cancel launch at any time

#### ⚙️ Settings Editor
- **Graphical Editor** — Edit D2R's `Settings.json` with a full GUI (50+ fields)
- **Presets** — Apply Low / Mid / High graphics presets instantly

#### 📊 Hardware Monitor
- Real-time CPU / GPU / Memory / VRAM monitoring (1s interval)

#### 🐱 Bongo Cat
- Animated desktop pet window (always-on-top), with skin system and loot drop animations

#### 🪟 Overlay Window
- Compact overlay shown when main window is minimized, with HW monitor and account status

#### 🎨 Theme System
- **Onyx Dark** and **Light** themes, system tray toggle, window geometry persistence

#### 🔄 Auto-Update
- Cloud version check on startup, one-click download and install

### Installation

**Prerequisites:** Windows 10/11, Diablo II: Resurrected installed via Battle.net, Microsoft Edge or Google Chrome.

Download the latest installer from the [Releases](https://github.com/gjy991229/D2rHub/releases) page.

**First run:** The setup wizard will auto-detect Battle.net path, D2R game directory, saved games folder, and browser — just click **Save & Continue**.

### Tech Stack

Tauri v2 / React 19 + TypeScript / Tailwind CSS v4 / Zustand / Vite 6

---

## 中文

D2Hub 是一款 Windows 桌面 GUI 工具，用于管理多个《暗黑破坏神 II：重制版》账号。它自动化了多开工作流——启动游戏实例、管理账号配置、应用画质预设，并处理注册表、符号链接和进程句柄等底层操作。

### 功能

#### 🎮 账号管理
- **账号增删改** — 创建、重命名、删除账号，支持拖拽排序
- **账号初始化** — 一键采集账号快照（配置、注册表、存档）
- **独立 Mod 参数** — 为每个账号单独设置 `-mod` 启动参数

#### 🚀 启动引擎
- **批量启动** — 一键顺序启动多个账号
- **仅启动战网** — 只启动 Battle.net，不启动游戏
- **配置隔离** — 按账号复制配置、创建符号链接、导入导出注册表
- **互斥体清理** — 清除进程句柄，实现多开
- **连接检测** — 实时检测游戏连接状态
- **取消支持** — 随时取消启动流程

#### ⚙️ 设置编辑器
- **图形化编辑** — 通过完整 GUI 编辑 D2R 的 `Settings.json`（50+ 配置项）
- **预设方案** — 一键应用低/中/高三档画质预设

#### 📊 硬件监控
- 实时 CPU / GPU / 内存 / 显存监控（1 秒刷新）

#### 🐱 Bongo Cat 桌面宠物
- 动画伴窗（置顶显示），支持皮肤系统和掉落动画

#### 🪟 迷你覆盖窗
- 主窗口最小化时显示紧凑覆盖窗，展示硬件监控和账号状态

#### 🎨 主题系统
- **暗色主题** 和 **亮色主题**，系统托盘切换，窗口位置记忆

#### 🔄 自动更新
- 启动时云端版本检查，一键下载安装更新

### 安装

**前置要求：** Windows 10/11，通过战网安装《暗黑破坏神 II：重制版》，Microsoft Edge 或 Google Chrome 浏览器。

从 [Releases](https://github.com/gjy991229/D2rHub/releases) 页面下载最新安装包。

**首次运行：** 设置向导会自动检测战网路径、D2R 游戏目录、存档文件夹和浏览器——只需点击 **保存并继续** 即可使用。

---

<div align="center">
  <p>
    <a href="https://github.com/gjy991229/D2rHub/issues">Report Bug</a> ·
    <a href="https://github.com/gjy991229/D2rHub/discussions">Discussion</a>
  </p>
</div>
