# 运行与部署说明

## 主服务

主服务入口位于 `adss-server` crate。默认监听 `127.0.0.1:8080`，可通过 `ADSS_BIND_ADDR` 覆盖。

```powershell
cargo run -p adss-server
```

```powershell
$env:ADSS_BIND_ADDR = "0.0.0.0:8080"
cargo run -p adss-server
```

当前主服务使用内存状态实现，用于固定 API、同步协议和审计契约。生产化接入 PostgreSQL 时应保持现有 contract 不变，把内存 store 替换为持久化 repository。

## Agent

Agent 入口位于 `adss-agent` crate。Agent 从环境变量读取主服务地址、域 ID 和 Agent ID。

```powershell
$env:ADSS_SERVER_URL = "http://127.0.0.1:8080"
$env:ADSS_DOMAIN_ID = "domain-a"
$env:ADSS_AGENT_ID = "agent-a"
$env:ADSS_AGENT_DRY_RUN = "1"
cargo run -p adss-agent
```

当前 Agent 只允许 `ADSS_AGENT_DRY_RUN=1` 运行，使用 dry-run directory client 执行一轮 `poll -> reconcile/password -> report`。真实部署前必须接入 LDAPS directory client，并移除 dry-run 强制保护。

## 当前外部依赖边界

- PostgreSQL：尚未接入，当前以内存 store 固定领域和 API 契约。
- mTLS：尚未接入传输层，当前通过 Agent 与域绑定逻辑固定授权行为。
- KMS/HSM：尚未接入，当前密码密封函数只保留不泄露明文的接口行为。
- LDAPS：尚未接入真实 AD，当前由 `DirectoryClient` trait 隔离，Agent runtime 已能按顺序调用结构操作和密码任务。

这些边界不能在部署说明中宣称已具备生产能力。下一阶段应优先把 PostgreSQL repository、真实 mTLS 和 LDAPS client 分别作为独立可验证提交推进。
