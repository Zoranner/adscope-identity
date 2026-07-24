# 运行与部署说明

## 配置文件

`adss-server` 和 `adss-agent` 是两个独立部署单元。每个进程启动时读取当前运行目录下的 `.env`；运行目录是进程启动时的工作目录，不要求放在仓库根目录。部署时分别复制对应示例文件：

```text
cp crates/adss-server/.env.example <server-runtime-dir>/.env
cp crates/adss-agent/.env.example <agent-runtime-dir>/.env
```

示例文件：

- [adss-server .env 示例](../../crates/adss-server/.env.example)
- [adss-agent .env 示例](../../crates/adss-agent/.env.example)

系统环境变量优先级高于运行目录下的 `.env`。生产环境可以由进程管理器、容器平台或 Secret 管理系统注入环境变量；不要把生产密钥写入仓库。

示例文件已说明每个变量的用途和是否必填。本文档只说明启动方式和部署边界，避免重复维护变量说明。

## 主服务

主服务是中心 API 和同步控制面。主服务必须配置 `ADSS_DATABASE_URL`、`ADSS_PASSWORD_ENCRYPTION_KEY`、`ADSS_PASSWORD_HASH_PROVIDER`、`ADSS_USER_SESSION_KEY` 和 `ADSS_MANAGEMENT_TOKEN`。

`ADSS_PASSWORD_ENCRYPTION_KEY` 是主服务内置密码加密使用的高熵密钥。该密钥通过受限 `.env`、系统环境变量、Windows DPAPI 或同等级本机 Secret 保护，不和数据库备份放在同一位置。

## 域配置初始化

域配置变更属于受控运维动作，应通过受保护管理入口维护。测试或部署初始化可以通过受控初始化脚本预置域记录，并写入 Agent key 摘要。

直接数据库写入只适用于初始化或迁移，不作为普通管理后台入口。

## Agent

Agent 是域内常驻同步进程。默认 [adss-agent .env 示例](../../crates/adss-agent/.env.example) 使用 dry-run，不写入 AD。

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

Agent 在运行目录下自动维护 `adss-agent-state.json`。文件无法解析时，Agent 可以以 `0/0` 进度和 rebuild flags 重新拉取，并在 confirm 被中心接受后覆盖 state。

## 部署要求

进入真实环境前必须满足：

- 主服务放在 TLS 后面，凭据响应禁止明文 HTTP。
- 生产环境配置本机高熵密码加密密钥。
- `ADSS_PASSWORD_ENCRYPTION_KEY`、`ADSS_USER_SESSION_KEY`、`ADSS_MANAGEMENT_TOKEN`、Agent key、LDAP bind password 通过受限 `.env`、系统环境变量、Windows DPAPI 或等价机制注入。
- 管理入口使用独立保护，不能把 `/api/admin/*` 暴露给普通用户 token。
- 域内服务账号只授予镜像根和隔离 OU 范围内的必要权限。
- 受管用户禁止域内普通 Change Password，并通过 GPO 隐藏 `Ctrl+Alt+Del` 改密入口。
- AD 沙箱域验证 OU、用户、组、成员、禁用、隔离移动和 Reset Password 全链路，并覆盖实际采用的 `ldap://` 或 `ldaps://` 连接方式。
