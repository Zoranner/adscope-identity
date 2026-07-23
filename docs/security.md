# 安全边界

## 中心唯一改密

MVP 规定用户密码只能通过中心服务修改。域内 AD 的普通 Change Password 路径不作为事实源，也不会传播到其他域。

受管账号应在 AD 中禁止用户自行 Change Password，并通过 GPO 隐藏 `Ctrl+Alt+Del` 的“更改密码”入口。Agent 使用被委派的 Reset Password 权限设置中心下发的密码。

## Verifier 与 Ciphertext

`user_credentials.password_verifier` 用于中心登录和改密前校验。主服务生产配置使用 Argon2id PHC 字符串，它不能还原密码，也不下发给 Agent。

`user_credentials.password_ciphertext` 用于保存中心当前密码材料。服务端在 `/api/agent/sync` 内存中解封后组装 `CredentialEntry.plaintext_password`，Agent 立即用该明文设置 AD 密码。

主服务通过 password envelope provider 处理 `password_ciphertext`。`local` provider 仅允许本地开发和自动化测试；生产必须使用 `command` provider 对接 KMS/HSM 或等价密钥服务，并由外部密钥系统承担密钥托管、轮换、访问审计和硬件保护能力。

## Agent Key

`domains.agent_key_hash` 保存 Agent key 的 `sha256:` 摘要，不保存明文 key。请求时服务端对 `x-adss-agent-key` 做同样摘要，并使用常量时间比较。

Agent key 仍是 MVP 共享密钥方案，不等价于 mTLS。部署时应通过 Windows DPAPI、Secret Manager、受限配置文件或等价机制保存 Agent key，禁止进入日志、错误响应、审计详情或配置仓库。

## 普通用户会话

普通用户登录成功后，主服务返回服务端签名的 Bearer token。token 只用于 `/api/me` 自助接口，并从签名内容中确定当前 `employee_id`；普通用户不能通过路径参数选择其他用户。

`/api/me/contact` 只允许普通用户修改自己的 `email`、`mobile` 和 `telephone`。`employee_id`、`username`、显示名、组织归属、启用状态和组成员不接受普通用户自助修改。

`/api/me/password` 根据 token 中的 `employee_id` 修改当前用户自己的密码；请求仍必须提供当前密码，服务端用 `password_verifier` 校验后才写入新的 verifier 和 ciphertext。

`ADSS_USER_SESSION_KEY` 是生产密钥材料，必须由 Secret Manager、KMS 派生密钥或等价机制注入，不得进入源码、日志或配置仓库。该 token 当前是 MVP 的服务端签名令牌，不等价于完整 SSO、OIDC 或复杂会话治理。

## 凭据传输

凭据响应会包含 Agent 可执行的明文密码，因此必须满足：

- 只允许通过 TLS 调用 `/api/agent/sync`。
- 成功响应设置 `Cache-Control: no-store`。
- 禁止在代理、网关、日志、tracing、错误回显、崩溃 dump 中记录响应体。
- Agent 不把明文密码写入本地 state 或日志。
- Agent 设置密码后立即丢弃明文。

当前自动化测试只能验证协议、编码、映射和错误处理，不代表 TLS、日志链路、代理缓存和真实域控权限已经满足生产要求。

## AD 权限

Agent 访问域控必须使用 LDAPS。域内服务账号应采用最小权限委派，只允许管理镜像根和隔离 OU 内的目标对象，并只授予需要的创建、移动、属性写入、组成员写入、禁用和 Reset Password 权限。

域控高权限凭据不得集中存放在主服务。主服务只保存中心业务事实、域同步配置和 Agent 认证摘要。

## 延后能力

以下能力不属于当前 MVP 主链：

- Agent 注册令牌。
- mTLS 客户端证书绑定。
- Agent key 轮换和吊销。
- 完整审计平台。
- drift 生命周期管理。
