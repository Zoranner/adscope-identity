# 运行与部署说明

## 配置文件

运行配置从根目录 `.env` 读取。复制 [.env.example](../../.env.example) 后按本机环境修改：

```text
cp .env.example .env
```

系统环境变量优先级高于 `.env`。生产环境可以由进程管理器、容器平台或 Secret 管理系统注入环境变量；不要把生产密钥写入仓库。

`.env.example` 已说明每个变量的用途、是否必填和适用进程。本文档只说明启动方式和部署边界，避免重复维护变量说明。

## 主服务

主服务入口位于 `adss-server` crate。主服务必须配置 `ADSS_DATABASE_URL`，并使用 password envelope provider 和 password hash provider。

启动命令：

```text
cargo run -p adss-server
```

生产环境使用 `ADSS_PASSWORD_ENVELOPE_PROVIDER=command` 对接 KMS/HSM 或等价密钥服务。`local` provider 只允许本地开发和自动化测试。

## 域配置初始化

主服务 schema 包含：

- `sync_metadata`
- `organizational_units`
- `users`
- `groups`
- `user_credentials`
- `domains`

域配置变更属于受控运维动作，应通过受保护管理入口维护。测试或部署初始化可以通过 repository、迁移脚本或运维脚本预置 `domains` 记录，并写入 `agent_key_hash`。

直接数据库写入只适用于初始化或迁移，不作为普通管理后台入口。

## Agent

Agent 入口位于 `adss-agent` crate。默认 `.env.example` 使用 dry-run，不写入 AD。

启动命令：

```text
cargo run -p adss-agent
```

启用真实域控写入时，设置：

```text
ADSS_AGENT_DRY_RUN=0
ADSS_LDAP_URL=ldap://dc-a.example.com:389
ADSS_LDAP_BIND_DN=CN=adss-agent,OU=Service Accounts,DC=a,DC=example,DC=com
ADSS_LDAP_BIND_PASSWORD=<from-local-secret>
```

Agent 访问域控支持 `ldap://` 或 `ldaps://`。生产环境建议使用 `ldaps://`，或仅在受保护网络内使用 `ldap://`；如果域控策略要求加密密码修改，应按域策略启用 `ldaps://` 或等价受保护绑定。

本地 state 文件只保存：

```json
{
  "applied_directory_revision": 0,
  "applied_credential_revision": 0
}
```

文件无法解析时，Agent 可以以 `0/0` 进度和 rebuild flags 重新拉取，并在 confirm 被中心接受后覆盖 state。

## 部署要求

进入真实环境前必须满足：

- 主服务放在 TLS 后面，凭据响应禁止明文 HTTP。
- 生产环境使用 `command` envelope provider，对接 KMS/HSM 或等价密钥服务。
- `ADSS_USER_SESSION_KEY`、Agent key、LDAP bind password 通过 Secret Manager、KMS、Windows DPAPI 或等价机制注入。
- 管理入口使用独立保护，不能把 `/api/admin/*` 暴露给普通用户 token。
- 域内服务账号只授予镜像根和隔离 OU 范围内的必要权限。
- 受管用户禁止域内普通 Change Password，并通过 GPO 隐藏 `Ctrl+Alt+Del` 改密入口。
- AD 沙箱域验证 OU、用户、组、成员、禁用、隔离移动和 Reset Password 全链路，并覆盖实际采用的 `ldap://` 或 `ldaps://` 连接方式。

## 验证命令

Rust 代码修改后执行：

```text
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
```

文档修改至少检查链接、标题、接口路径和术语一致性。文档检查不代表真实 AD、TLS、KMS/HSM 或生产权限已经验收。
