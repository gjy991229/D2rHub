# D2R Token 启动完成判定研究

## 结论

Token 启动流程不应依赖固定远端端口。注册表 `WEB_TOKEN` 的阶段性变化可以作为最快、语义最强的判据；但实测表明它不能作为唯一判据，必须保留由目标游戏窗口和稳定网络状态共同构成的兜底条件。

D2R 使用同一个用户级注册表值：

```text
HKCU\SOFTWARE\Blizzard Entertainment\Battle.net\Launch Options\OSI\WEB_TOKEN
```

开源多开器 Diablo2RLoader 的当前实现和说明都明确指出，该值会在游戏启动和到达角色选择界面时更新。它在启动阶段的一次变化已经发生后，读取当前二进制值作为基线；随后每 451 ms 重读一次，检测到下一次变化后才认为可以启动下一个账号。固定源码证据见：[写入 `WEB_TOKEN`](https://github.com/shupershuff/Diablo2RLoader/blob/4a0ec88136970bd1e4f58d708ee4f7cb9391cd6b/D2Loader.ps1#L5035-L5055)和[等待角色选择阶段的下一次变化](https://github.com/shupershuff/Diablo2RLoader/blob/4a0ec88136970bd1e4f58d708ee4f7cb9391cd6b/D2Loader.ps1#L5365-L5382)。同一作者的无脚本说明也记录了 `WEB_TOKEN` 在启动、到达角色选择界面和退出时会更新，并要求到达角色选择界面后才能启动下一账号：[Auth Token 启动说明](https://github.com/shupershuff/D2r-Multiboxing-Methods/blob/5d41f6e2ea6125a2ccca5c6b5c4fa7a39ce2b3f1/README.md#L37-L76)。

因此，D2rHub 优先判断“共享 `WEB_TOKEN` 已进入角色选择阶段，可以被下一账号覆盖”。如果当前客户端没有产生可观测的两次回写，则在目标游戏窗口已经出现后，以目标 PID 连续多次存在 `ESTABLISHED` 连接作为启动就绪的保守兜底；单独出现窗口或单独存在网络连接都不能放行。

### 实测修正（2026-08-26）

在当前国际服客户端的一键启动实测中，游戏已经成功登录并进入大厅，但 D2rHub 仍停留在“等待 `WEB_TOKEN` 角色选择阶段回写”直至超时。这说明当前客户端可能不产生两次可观测回写、两次变化可能在轮询间隔内合并，或当前流程只产生一次变化。无论具体属于哪一种情况，“必须观测到两次注册表变化”都会产生假阴性，因此不能继续作为唯一完成条件。

## 信号比较

| 信号 | 能证明什么 | 主要问题 | 建议 |
| --- | --- | --- | --- |
| 固定远端 TCP 1119 | 目标进程恰好存在一条该端口连接 | 国际服当前连接并不保证使用 1119；代理、线路和协议变化都会造成假阴性 | 移除为完成条件 |
| 固定远端 TCP 443 | D2R 有一条常见 Battle.net/D2R HTTPS 风格连接 | 只能证明联网，不能区分认证、大厅、CDN 或角色选择完成 | 仅作诊断或弱兜底 |
| 按 PID 的任意 `ESTABLISHED` TCP | 目标 D2R 进程至少有可传输数据的 TCP 连接 | 语义过宽；认证、遥测、CDN、代理连接都可能提前满足 | 仅与目标窗口出现及连续多次采样组合为兜底条件 |
| 进程、窗口出现或稳定时长 | 客户端 UI 已初始化到某个程度 | 离线界面、错误弹窗和在线大厅都可能满足 | 仅与稳定网络采样组合为兜底条件 |
| `WEB_TOKEN` 角色选择阶段变化 | D2R 已推进到会改写共享 Token 注册表状态的阶段 | 当前客户端不保证能观测到预期的两次变化 | Token 流程的快速完成条件，不作为唯一条件 |

## 端口证据

当前开源的 D2R Counter 把游戏会话候选流量筛在 TCP 443，并进一步核对本地端口属于 `D2R.exe` 的 PID：[端口常量与 PID 归属检查](https://github.com/remoniker/d2r-counter/blob/552800eef3df829c005d74f4ab946da05c6555ae/src/main.py#L84-L86)、[`psutil.net_connections` 的 PID 核对](https://github.com/remoniker/d2r-counter/blob/552800eef3df829c005d74f4ab946da05c6555ae/src/main.py#L142-L155)。这说明 443 比 1119 更符合该工具采集到的当前 D2R 游戏流量。

但它没有把“看到 443”直接等同于进入游戏。对已经存在的 443 连接，它只标记为推断的 Battle.net 连接：[连接检查](https://github.com/remoniker/d2r-counter/blob/552800eef3df829c005d74f4ab946da05c6555ae/src/main.py#L641-L656)；真正进入游戏还要通过数据方向、突发流量等分类器，[项目说明明确提到需要区分 auth、lobby 和 CDN 的假阳性](https://github.com/remoniker/d2r-counter/blob/552800eef3df829c005d74f4ab946da05c6555ae/README.md#L229-L239)。所以把 D2rHub 的 1119 改成 443 能减少部分假阴性，却仍不是可靠的 Token 完成判据。

Windows 官方 API 也没有赋予端口表“登录阶段”的业务语义。`GetExtendedTcpTable` 只返回 TCP endpoint 表，并要求分别以 `AF_INET` 和 `AF_INET6` 查询 IPv4/IPv6；`TCP_TABLE_OWNER_PID_*` 才会返回带 PID 的表：[Microsoft `GetExtendedTcpTable`](https://learn.microsoft.com/en-us/windows/win32/api/iphlpapi/nf-iphlpapi-getextendedtcptable)。`MIB_TCP_STATE_ESTAB` 只表示连接已打开、处于正常数据传输状态：[Microsoft `MIB_TCP6ROW_OWNER_PID`](https://learn.microsoft.com/en-us/windows/win32/api/tcpmib/ns-tcpmib-mib_tcp6row_owner_pid)。因此，即使补齐 IPv6，也只能提高网络观测完整性，不能证明已进入角色选择阶段。

## 加速器和代理的影响

固定远端端口在加速器下尤其脆弱。Windows Filtering Platform 可以把应用原本的连接重定向到本地代理服务，由代理另建一条到最终目的地的连接；这会改变应用直接可见的 endpoint 和连接所有者：[Microsoft WFP Bind/Connect Redirection](https://learn.microsoft.com/en-us/windows-hardware/drivers/network/using-bind-or-connect-redirection)。因此，D2R.exe 可能只直接连接本地代理的任意端口，真正的 443/1119 出站连接则属于加速器进程。注册表变化不依赖网络路径，天然规避这一类差异。

## 推荐状态机

针对 Token 启动：

1. 将账号 Token 写入 `WEB_TOKEN`，保留写入后的原始 `REG_BINARY` 快照。
2. 启动目标 `D2R.exe`，确认新 PID。
3. 从写入完成起跟踪 `WEB_TOKEN` 原始字节变化；若观测到两个阶段的变化，立即按注册表快速条件放行。
4. 启动 2 秒后同时检查目标 PID 的可见游戏窗口和网络状态；只有窗口已出现且连续 4 次、每次间隔 500 ms 都存在目标 PID 的 `ESTABLISHED` 连接，才按兜底条件放行。任意一次网络采样失败都将连续计数清零。
5. 两个条件任一满足即可启动下一个账号：注册表变化是快速路径，窗口与稳定网络的组合是兼容当前国际服客户端的兜底路径。
6. 保留 60 秒超时和取消检查。超时时记录 PID 是否存活、观测到几次 Token 变化和连续网络采样次数，便于区分认证失败、区域故障和信号丢失。Diablo2RLoader 的上游循环没有超时，不应照搬这一点。
7. Battle.net 客户端启动分支可继续使用网络/窗口就绪判据，但应去掉固定 1119，并把 IPv4、IPv6 与代理场景纳入考虑。

更事件化的实现可以在启动前对 OSI 注册表项注册 `RegNotifyChangeKeyValue(REG_NOTIFY_CHANGE_LAST_SET)`，收到通知后重读并比较 `WEB_TOKEN` 原始字节。Microsoft 说明该 API 能通知注册表项内容变化，异步模式通过事件对象报告，而且每次通知后需要重新注册：[Microsoft `RegNotifyChangeKeyValue`](https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-regnotifychangekeyvalue)。由于通知是“键发生变化”而不是“指定值变化”，仍必须重读 `WEB_TOKEN` 并比较字节；当前每 0.5–1 秒轮询已足够简单，事件通知可作为后续优化。

## 实现风险

- 轮询基线取得过晚：极快机器可能在取基线前已经完成角色选择阶段变化，随后一直等到退出时才再次变化。最稳妥的方式是在启动前建立变化观察并记录两个阶段；最小修复应至少尽早取得基线。
- 多实例并发写同一个 `WEB_TOKEN`：任意其他 D2R 启动或退出都会改变相同值，所以 Token 批量启动仍必须串行；等待期间应提示用户不要手动启动或关闭另一个实例。
- 注册表读取失败：应视为检测错误并返回具体错误，而不是退化成“未连接”。
- 网络兜底必须同时查询 IPv4/IPv6、按目标 PID 过滤，不绑定 1119 或 443 等固定端口，并要求可见目标窗口和连续多次成功采样；它与注册表快速路径是“或”的关系，但内部的窗口与网络条件是“且”的关系。
