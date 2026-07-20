# 运行与部署说明

## 主服务环境变量

主服务入口位于 `adss-server` crate。MVP 必须使用数据库启动，缺少 `ADSS_DATABASE_URL` 会直接失败。

必需环境变量：

- `ADSS_DATABASE_URL`：SeaORM 数据库连接串，例如 `sqlite://adss.db?mode=rwc`。
- `ADSS_DEV_PASSWORD_ENCRYPTION_KEY`：MVP 开发 envelope 使用的密钥材料。该机制不是生产 KMS。

可选环境变量：

- `ADSS_BIND_ADDR`：监听地址，默认 `127.0.0.1:8080`。

示例：

```powershell
$env:ADSS_DATABASE_URL = "sqlite://adss.db?mode=rwc"
$env:ADSS_DEV_PASSWORD_ENCRYPTION_KEY = "local-dev-only-key"
$env:ADSS_BIND_ADDR = "127.0.0.1:8080"
cargo run -p adss-server
```

启动时主服务会初始化 MVP schema：

- `sync_metadata`
- `organizational_units`
- `users`
- `groups`
- `user_credentials`
- `domains`

当前没有域管理 API。测试或部署初始化需要通过 repository、迁移脚本或运维脚本预置 `domains` 记录，并写入 `agent_key_hash`。

## Agent 环境变量

Agent 入口位于 `adss-agent` crate。

必需环境变量：

- `ADSS_DOMAIN_ID`：本 Agent 绑定的域。
- `ADSS_AGENT_KEY`：该域的 Agent key 明文，由 Agent 本地 Secret 保存。
- `ADSS_AGENT_STATE_PATH`：本地 revision state 文件路径。
- `ADSS_AGENT_DRY_RUN=1`：当前必须启用 dry-run。

可选环境变量：

- `ADSS_SERVER_URL`：主服务地址，默认 `http://127.0.0.1:8080`。
- `ADSS_AGENT_INTERVAL_SECONDS`：轮询间隔秒数，默认 `60`，必须大于 `0`。

示例：

```powershell
$env:ADSS_SERVER_URL = "http://127.0.0.1:8080"
$env:ADSS_DOMAIN_ID = "domain-a"
$env:ADSS_AGENT_KEY = "domain-a-agent-key"
$env:ADSS_AGENT_STATE_PATH = ".\agent-domain-a-state.json"
$env:ADSS_AGENT_INTERVAL_SECONDS = "60"
$env:ADSS_AGENT_DRY_RUN = "1"
cargo run -p adss-agent
```

本地 state 文件只保存：

```json
{
  "applied_directory_revision": 0,
  "applied_credential_revision": 0
}
```

文件无法解析时，Agent 会以 `0/0` 进度和 rebuild flags 重新拉取，并在 confirm 被中心接受后覆盖 state。

## 当前运行能力

当前主链已经是 repository-backed MVP：

- 中心写入目录当前事实并推进目录 revision。
- 中心改密写入当前 verifier 和 ciphertext 并推进凭据 revision。
- Agent 定时 sync，分别执行目录和凭据通道。
- Agent 只有在 confirm `accepted=true` 后才保存本地 revision。

真实 LDAPS 尚未接入。Agent 仍使用 `DryRunDirectoryClient`，非 dry-run 启动会失败。当前不能宣称已经能修改真实 AD。

## 部署前置条件

进入真实环境前至少需要补齐：

- 主服务必须放在 TLS 后面，凭据响应禁止明文 HTTP。
- `ADSS_DEV_PASSWORD_ENCRYPTION_KEY` 必须替换为 KMS/HSM 或等价 envelope。
- Agent 需要真实 LDAPS DirectoryClient。
- 域内服务账号只授予镜像根和隔离 OU 范围内的必要权限。
- 受管用户应禁止域内普通 Change Password，并通过 GPO 隐藏 `Ctrl+Alt+Del` 改密入口。
