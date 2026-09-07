# D2R 启动完成判定研究

## 当前实现（2026-09-07）

Token 直启和 Battle.net 启动均并行检测 ETW 和 TCP 登录信号，任一命中即停止另一检测和跳过动画按键：

- ETW 信号：目标 `D2R.exe` PID 成功读取注册表值 `WEB_TOKEN`。
- TCP 信号：连续两次采样均存在属于目标 PID、远端端口为 `1119` 的 `ESTABLISHED` TCP 连接。
- Token 直启成功条件：任一登录信号命中，且该 PID 的多开互斥句柄已经成功关闭。

批量启动仍要求当前账号启动成功且互斥句柄已清除，才会继续启动下一个账号。

## ETW 判定

监听对象是 Windows ETW provider `Microsoft-Windows-Kernel-Registry`（GUID `70eb4f03-c1de-4f73-a051-33d13d5413bd`）的 `QueryValue` 事件（事件 ID 7）。事件必须同时满足：

- 事件 PID 等于刚启动的目标 D2R PID；
- `ValueName` 不区分大小写等于 `WEB_TOKEN`；
- `Status` 等于 0，表示查询成功。

这不是在读取或导出 Token 内容。监听器只保留满足条件的 PID、匹配事件数和解析错误数。

两种模式均在创建 D2R 前尝试启动监听，避免漏掉快速发生的读取事件。监听启动失败只记录告警并继续 TCP 检测，不会阻断游戏启动。

## ETW/TCP 竞争判定

ETW 命中目标 D2R PID 后立即停止 TCP 采样；TCP 先稳定时则立即停止 ETW 会话。

两种模式复用同一 TCP 检测，通过 Windows `GetExtendedTcpTable` 分别读取 IPv4 和 IPv6 TCP 表，并只接受：

- 连接所属 PID 等于本次新启动的目标 `D2R.exe` PID；
- TCP 状态为 `ESTABLISHED`；
- 远端端口唯一匹配 `1119`；
- 连续两次、间隔约 1 秒的采样均命中。

该判断只将 `1119` 视为大厅端口，目标进程建立的 443 或其他端口连接不会使 TCP 检测就绪；一次未命中会重置连续采样计数。检测期间继续向游戏窗口发送跳过动画按键；ETW 或 TCP 任一命中后停止发送。

Token 直启并行处理 `DiabloII Check For Other Instances` 互斥句柄清除，在同一个 60 秒等待窗口内要求登录信号和句柄清除均就绪；登录信号先命中时只继续等待句柄清除。超时日志分别记录 ETW、TCP 和互斥句柄状态，成功、取消或失败时停止监控。Battle.net 模式在登录信号命中后额外等待最多 3 秒确认互斥句柄处理结果。

TCP `ESTABLISHED` 是兼容性联网信号，不能严格证明认证 Token 已消费或认证已经成功；互斥句柄清除也只证明多开限制已解除。

## 已知风险与验证点

- 两种模式在国服、国际服以及常用加速器环境下，均依赖目标 D2R PID 保留远端端口 `1119` 的大厅连接来命中 TCP。
- 如果加速器由其他进程持有游戏连接，或将目标进程的远端端口改为非 `1119`，严格 PID/端口过滤不会命中 TCP；此时可由 ETW 路径兜底。
- 两种模式启动 Kernel Registry provider 均需要相应权限；ETW 启动失败会自动保留 TCP 路径。
- 当前假设 `WEB_TOKEN` 由 `D2R.exe` 自身读取。如果事件由中间辅助进程发出，严格 PID 过滤不会命中 ETW；两种模式均可由 TCP 兜底。
