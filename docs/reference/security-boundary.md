# 安全实现边界

## 密码事实源

用户密码只能通过中心服务修改。域内 AD 的普通 Change Password 路径不作为事实源，也不会传播到其他域。

受管账号应禁止用户在 AD 中自行 Change Password，并通过 GPO 隐藏 `Ctrl+Alt+Del` 的“更改密码”入口。Agent 使用被委派的 Reset Password 权限设置中心下发的密码。

## Verifier 与 Ciphertext

`user_credentials.password_verifier` 用于中心登录和改密前校验。生产配置使用 Argon2id PHC 字符串；verifier 不能还原密码，也不下发给 Agent。

`user_credentials.password_ciphertext` 用于保存中心当前密码材料。主服务通过密码加密方式处理密文，只有在响应 `/api/agent/sync` 时才在内存中解封，并组装 `CredentialEntry.plaintext_password`。

主服务使用内置 XChaCha20-Poly1305 加密密码材料，并通过本机受保护配置提供高熵 `ADSS_PASSWORD_ENCRYPTION_KEY`。该密钥必须和数据库备份分离保存，不能进入源码、日志或配置仓库。

## 凭据传输

凭据响应包含 Agent 可执行的明文密码，必须满足以下要求：

- 只允许通过主服务 TLS 调用 `/api/agent/sync`。
- 成功响应设置 `Cache-Control: no-store`。
- 代理、网关、日志、tracing、错误回显和崩溃 dump 不记录响应体。
- Agent 不把明文密码写入本地 state、日志或配置。
- Agent 设置密码后立即丢弃明文。

自动化测试只能验证协议、编码、映射和错误处理，不能替代 TLS、日志链路、代理缓存和真实域控权限验收。

## Agent Key

`domains.agent_key_hash` 保存 Agent key 的 `sha256:` 摘要，不保存明文 key。请求时服务端对 `x-adss-agent-key` 做同样摘要，并使用常量时间比较。

Agent key 是域绑定共享密钥。部署时应通过 Windows DPAPI、Secret Manager、受限配置文件或等价机制保存 Agent key。Agent key 不得进入日志、错误响应或配置仓库。

## 普通用户会话

普通用户登录成功后，主服务返回服务端签名的 Bearer token。token 只用于 `/api/me/*` 自助接口，并从签名内容中确定当前 `employee_id`。

普通用户不能通过路径参数选择其他用户，不能修改 `employee_id`、`username`、显示名、组织归属、启用状态、组成员或域配置。

`/api/me/password` 必须根据 token 中的 `employee_id` 修改当前用户自己的密码。请求必须提供当前密码，服务端用 `password_verifier` 校验后才写入新的 verifier 和 ciphertext。

`ADSS_USER_SESSION_KEY` 和 `ADSS_MANAGEMENT_TOKEN` 是生产密钥材料，必须通过受限 `.env`、系统环境变量、Secret Manager、Windows DPAPI 或等价机制注入，不得进入源码、日志或配置仓库。

## 管理入口

管理入口必须位于受保护入口后，且必须和普通用户自助 token 区分。

域、OU、用户、组、管理员代设密码和同步状态查询接口都属于管理面。管理员代设密码不要求用户当前密码，因此不能复用普通用户自助改密的安全语义。

`/api/admin/*` 不得在缺少管理入口保护的情况下暴露给公网或普通用户入口。

## AD 权限

Agent 访问域控支持 `ldap://` 或 `ldaps://`。生产环境建议使用 `ldaps://`，或仅在受保护网络内使用 `ldap://`；如果域控策略要求加密密码修改，应按域策略启用 `ldaps://` 或等价受保护绑定。

域内服务账号应采用最小权限委派，只允许管理镜像根和隔离 OU 内的目标对象，并授予必要的创建、移动、属性写入、组成员写入、禁用和 Reset Password 权限。

域控高权限凭据不得集中存放在主服务。主服务只保存中心业务事实、域同步配置和 Agent 认证摘要。
