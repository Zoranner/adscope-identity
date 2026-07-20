# 运行与部署说明

## 主服务环境变量

主服务入口位于 `adss-server` crate。MVP 必须使用数据库启动，缺少 `ADSS_DATABASE_URL` 会直接失败。

必需环境变量：

- `ADSS_DATABASE_URL`：SeaORM 数据库连接串，例如 `sqlite://adss.db?mode=rwc`。
- `ADSS_PASSWORD_ENVELOPE_PROVIDER`：密码 envelope provider，支持 `local` 和 `command`。

`local` provider 仅用于本地开发和自动化测试：

- `ADSS_PASSWORD_ENVELOPE_LOCAL_KEY`：本地 envelope key，不得用于生产。

`command` provider 用于生产对接 KMS/HSM 或等价密钥服务：

- `ADSS_PASSWORD_ENVELOPE_COMMAND`：本机 envelope 适配器可执行文件路径。服务端以参数 `seal` 或 `open` 调用该命令，通过 stdin 传入明文或密文，通过 stdout 读取结果。

可选环境变量：

- `ADSS_BIND_ADDR`：监听地址，默认 `127.0.0.1:8080`。

示例：

```powershell
$env:ADSS_DATABASE_URL = "sqlite://adss.db?mode=rwc"
$env:ADSS_PASSWORD_ENVELOPE_PROVIDER = "local"
$env:ADSS_PASSWORD_ENVELOPE_LOCAL_KEY = "local-dev-only-key"
$env:ADSS_BIND_ADDR = "127.0.0.1:8080"
cargo run -p adss-server
```

生产 envelope 示例：

```powershell
$env:ADSS_DATABASE_URL = "postgres://adss:<secret>@db/adss"
$env:ADSS_PASSWORD_ENVELOPE_PROVIDER = "command"
$env:ADSS_PASSWORD_ENVELOPE_COMMAND = "C:\Program Files\ADSS\adss-envelope-kms.exe"
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

可选环境变量：

- `ADSS_SERVER_URL`：主服务地址，默认 `http://127.0.0.1:8080`。
- `ADSS_AGENT_INTERVAL_SECONDS`：轮询间隔秒数，默认 `60`，必须大于 `0`。
- `ADSS_AGENT_DRY_RUN`：设置为 `1` 或 `true` 时只跑协议和本地 state，不写入 AD。

非 dry-run 必需环境变量：

- `ADSS_LDAP_URL`：域控 LDAPS 地址，例如 `ldaps://dc-a.example.com:636`。
- `ADSS_LDAP_BIND_DN`：Agent 服务账号 DN。
- `ADSS_LDAP_BIND_PASSWORD`：Agent 服务账号密码，应由本机 Secret 注入。
- `ADSS_LDAP_ACCEPT_INVALID_CERTS`：仅测试环境可设为 `1` 或 `true`，生产不得启用。

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

LDAPS 示例：

```powershell
$env:ADSS_SERVER_URL = "https://sync.example.com"
$env:ADSS_DOMAIN_ID = "domain-a"
$env:ADSS_AGENT_KEY = "domain-a-agent-key"
$env:ADSS_AGENT_STATE_PATH = ".\agent-domain-a-state.json"
$env:ADSS_AGENT_INTERVAL_SECONDS = "60"
$env:ADSS_LDAP_URL = "ldaps://dc-a.example.com:636"
$env:ADSS_LDAP_BIND_DN = "CN=adss-agent,OU=Service Accounts,DC=a,DC=example,DC=com"
$env:ADSS_LDAP_BIND_PASSWORD = "<from-local-secret>"
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
- 主服务通过 password envelope provider 保存和打开当前密码材料。
- Agent 定时 sync，分别执行目录和凭据通道。
- Agent 非 dry-run 时通过 LDAPS 创建和更新 OU、用户、组、成员关系、禁用状态、隔离 OU 移动和密码。
- Agent 只有在 confirm `accepted=true` 后才保存本地 revision。

当前代码已接入 LDAPS 客户端，但本仓库验证只能覆盖协议、映射、编码和编译测试。真实域控写入需要在 AD 沙箱环境执行端到端验证后才能作为生产能力验收。

## 部署前置条件

进入真实环境前至少需要补齐：

- 主服务必须放在 TLS 后面，凭据响应禁止明文 HTTP。
- 生产环境必须使用 `command` envelope provider，并由该命令对接 KMS/HSM 或等价密钥服务；不得使用 `local` provider。
- 域内服务账号只授予镜像根和隔离 OU 范围内的必要权限。
- 受管用户应禁止域内普通 Change Password，并通过 GPO 隐藏 `Ctrl+Alt+Del` 改密入口。
- 需要在 AD 沙箱域验证 OU、用户、组、成员、禁用、隔离移动和 Reset Password 全链路。
