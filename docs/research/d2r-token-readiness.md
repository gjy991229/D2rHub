# D2R 启动完成判定研究

## 当前实现（2026-08-30）

Token 直启和 Battle.net 启动使用不同的组合判据：

- Token 直启：目标 `D2R.exe` PID 成功读取注册表值 `WEB_TOKEN`，并且该 PID 的多开互斥句柄已经成功关闭。
- Battle.net 启动：并行检测目标 PID 读取 `WEB_TOKEN` 的 ETW 事件，以及连续两次采样均存在远端端口为 `1119` 的 `ESTABLISHED` TCP 连接；任一信号先命中即停止另一检测。

批量启动仍要求当前账号启动成功且互斥句柄已清除，才会继续启动下一个账号。

## Token 直启的 ETW 判定

监听对象是 Windows ETW provider `Microsoft-Windows-Kernel-Registry`（GUID `70eb4f03-c1de-4f73-a051-33d13d5413bd`）的 `QueryValue` 事件（事件 ID 7）。事件必须同时满足：

- 事件 PID 等于刚启动的目标 D2R PID；
- `ValueName` 不区分大小写等于 `WEB_TOKEN`；
- `Status` 等于 0，表示查询成功。

这不是在读取或导出 Token 内容。监听器只保留满足条件的 PID、匹配事件数和解析错误数。

Token 直启在创建 D2R 前启动监听，避免漏掉快速发生的读取事件。随后并行检测 Token 消费和 `DiabloII Check For Other Instances` 互斥句柄清除；60 秒仍未同时满足则失败。

## Battle.net 的 ETW/TCP 竞争判定

Battle.net 在发送游戏启动指令前尝试启动 ETW 监听。监听启动失败只记录告警并继续 TCP 检测，不会阻断战网启动。ETW 命中目标 D2R PID 后立即停止 TCP 采样；TCP 先稳定时则立即停止 ETW 会话。

Battle.net 路径通过 Windows `GetExtendedTcpTable` 分别读取 IPv4 和 IPv6 TCP 表，并只接受：

- 连接所属 PID 等于本次新启动的目标 `D2R.exe` PID；
- TCP 状态为 `ESTABLISHED`；
- 远端端口唯一匹配 `1119`；
- 连续两次、间隔约 1 秒的采样均命中。

该判断只将 `1119` 视为大厅端口，目标进程建立的 443 或其他端口连接不会使 TCP 检测就绪。检测期间继续向游戏窗口发送跳过动画按键；ETW 或 TCP 任一命中后停止发送，并额外等待最多 3 秒确认互斥句柄处理结果。

TCP `ESTABLISHED` 只能作为 Battle.net 模式的兼容性联网信号，不能严格证明认证 Token 已消费。遥测、CDN、代理或大厅连接可能提前满足；ETW 与互斥句柄检测提供额外保障。

## 已知风险与验证点

- Battle.net 模式需要分别验证国服、国际服以及常用加速器环境，确认目标 D2R PID 仍保留远端端口 `1119` 的大厅连接。
- 如果加速器由其他进程持有游戏连接，或将目标进程的远端端口改为非 `1119`，严格 PID/端口过滤不会命中 TCP；此时可由 ETW 路径兜底。
- Token 直启仍依赖管理员权限启动 Kernel Registry provider；Battle.net 的 ETW 启动失败则自动保留 TCP 路径。
- 当前假设 `WEB_TOKEN` 由 `D2R.exe` 自身读取。如果事件由中间辅助进程发出，严格 PID 过滤不会命中 ETW；Battle.net 可由 TCP 兜底，Token 直启仍会超时。
