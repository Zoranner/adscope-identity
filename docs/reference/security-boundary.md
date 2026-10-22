# 安全实现边界

## 密码事实源

用户密码只能通过 Core 修改。域内 AD 的普通 Change Password 路径不作为事实源，也不会传播到其他域。

受管账号应禁止用户在 AD 中自行 Change Password，并通过 GPO 隐藏 `Ctrl+Alt+Del` 的“更改密码”入口。Syncer 使用被委派的 Reset Password 权限设置 Core 下发的密码。

## Verifier 与 Ciphertext

`user_credentials.password_verifier` 用于 Core 登录和改密前校验。生产配置使用 Argon2id PHC 字符串；verifier 不能还原密码，也不下发给 Syncer。

`user_credentials.password_ciphertext` 用于保存 Core 当前密码材料。Core 通过密码加密方式处理密文，只有在响应 `/api/syncer/sync` 时才在内存中解封，并组装 `CredentialEntry.plaintext_password`。

Core 使用内置 XChaCha20-Poly1305 加密密码材料，并通过本机受保护配置提供高熵 `PASSWORD_ENCRYPTION_KEY`。该密钥必须和数据库备份分离保存，不能进入源码、日志或配置仓库。

## 凭据传输

凭据响应包含 Syncer 可执行的明文密码，必须满足以下要求：

- 只允许通过 Core 的 TLS 调用 `/api/syncer/sync`。
- 成功响应设置 `Cache-Control: no-store`。
- Portal、日志、tracing、错误回显和崩溃 dump 不记录响应体。
- Syncer 不把明文密码写入本地 state、日志或配置。
- Syncer 设置密码后立即丢弃明文。

自动化测试只能验证协议、编码、映射和错误处理，不能替代 TLS、日志链路、Portal 缓存和真实域控权限验收。

## Syncer Key

`domains.connector_key_hash` 是历史物理列名，保存 Syncer key 的 `sha256:` 摘要，不保存明文 key。请求时服务端对 `x-adscope-syncer-key` 做同样摘要，并使用常量时间比较。

Syncer key 是域绑定共享密钥。部署时应通过 Windows DPAPI、Secret Manager、受限配置文件或等价机制保存 Syncer key。Syncer key 不得进入日志、错误响应或配置仓库。

## 普通用户会话

普通用户登录成功后，Core 返回 HMAC-SHA256 签名的 Bearer token，并把同一个 token 写入 `adscope_sso` Cookie。Bearer token 只用于 `/api/me/*` 自助接口；Cookie 只用于浏览器 OIDC 授权流程。两种入口都从签名内容中确定当前 `employee_id`，不建立服务端会话表。

`adscope_sso` Cookie 使用 `HttpOnly`、`Secure`、`SameSite=Lax` 和 `Path=/`。默认有效期为一小时，可通过服务端会话 TTL 配置调整。`POST /api/auth/logout` 只清除当前浏览器 Cookie，不接受或撤销远程会话。

普通用户不能通过路径参数选择其他用户，不能修改 `employee_id`、`username`、显示名、组织归属、启用状态、组成员或域配置。

`/api/me/password` 必须根据 token 中的 `employee_id` 修改当前用户自己的密码。请求必须提供当前密码，服务端用 `password_verifier` 校验后才写入新的 verifier 和 ciphertext。

`SESSION_KEY` 和 `MANAGEMENT_TOKEN` 是生产密钥材料，必须通过受限 `.env`、系统环境变量、Secret Manager、Windows DPAPI 或等价机制注入，不得进入源码、日志或配置仓库。

## 管理入口

管理入口必须位于受保护入口后，且必须和普通用户自助 token 区分。

域、OU、用户、组、管理员代设密码和同步状态查询接口都属于管理面。管理员代设密码不要求用户当前密码，因此不能复用普通用户自助改密的安全语义。

`/api/admin/*` 不得在缺少管理入口保护的情况下暴露给公网或普通用户入口。

内网管理可以复用内网 Portal；外网入口不得暴露管理面。Core 只校验 `MANAGEMENT_TOKEN`，不判断请求来源网络；来源限制由 Portal、防火墙、VLAN 或 VPN 负责。

## OIDC 身份与 issuer

Core OIDC 复用现有用户名和密码校验，不复制账号或身份资料。OIDC `sub` 使用用户 `employee_id`；`profile`、`email` 和 `phone` 声明从用户表读取。OIDC access token 只授权访问 `/oauth2/userinfo`，不能代替普通用户 Bearer token、管理会话 Cookie 或 Syncer key。

`OIDC_ISSUERS` 是 Core 接受的 Portal issuer 白名单。每一项必须是仅包含 HTTPS scheme、host 和可选端口的外部 origin，不接受 userinfo、业务路径、query 或 fragment。Host 统一小写、去掉末尾点和默认 `:443`，非默认端口保留。Core 自己的后端地址不加入白名单，直接访问其 discovery、JWKS、authorize、token 或 userinfo 返回 `404`。Portal 用 `PUBLIC_ISSUER` 覆盖转发 Host，Core 不信任客户端 Host、`Forwarded` 或 `X-Forwarded-Host`。

Docker 中签名私钥固定从 `/run/secrets/oidc-private-key.pem` 读取；从仓库根目录本地调试时，Core 优先读取 `./secrets/oidc-private-key.pem`。密钥必须是至少 2048 位的 RSA PKCS#8 或 PKCS#1 PEM，不能写入数据库、源码、日志、镜像或发布包。根目录 `secrets/` 必须保持 Git 忽略。Core 使用 `RS256` 签发 ID Token 和 access token，JWKS 只发布当前公钥；服务端不保存历史签名密钥集合，也不提供管理端密钥轮换接口。

## OIDC 网络接入

Core 只做后端；每个 Portal 作为自身公开入口的 issuer。Portal 负责 TLS、静态前端、API/OIDC 转发和两组入口路径判断，不在 Portal 上实现 OIDC Broker 或本地签发 Token。

Portal 终止 TLS 时会看到密码、Cookie、client secret 和 Token 响应，必须禁止记录或缓存这些内容。Core 本身不管理 TLS 证书。Portal 可以自己终止 TLS，也可以由前置反向代理终止；使用 HTTP 监听时只能暴露在本机回环或受信网络。

不同公开 origin 的 `adscope_sso` Cookie 都没有 `Domain` 属性，独立登录和退出，不共享登录态。授权码和 access token 绑定当前 issuer，跨 Portal 兑换返回 `invalid_grant`，跨 Portal 调用 UserInfo 返回 `401 invalid_token`。Core 故障时，Portal 返回 502/503，不能在 Portal 本地签发 Token。

每个 Portal 通过 `LOGIN_ENABLED` 和 `ADMIN_ENABLED` 两个环境变量控制登录和管理。Portal 命中禁用路由时直接返回 `403` 错误页，不转发给 Core；Core 不参与开关判断。关闭登录不撤销已签发 Token，也不影响依赖方本地会话；关闭管理不影响用户登录和 Syncer 同步。受信内网 Portal 可以开启管理，面向不受信网络的 Portal 必须设置 `ADMIN_ENABLED=false`。

## OIDC 授权确认

浏览器已有有效 SSO Cookie 时仍要进入确认页。服务端不保存历史同意结果，`prompt=none` 固定返回 `interaction_required`，不能用于静默签发授权码。

确认页通过 `/api/oauth2/authorize/context` 取得短期 CSRF token。该 token 使用 HMAC-SHA256 签名，绑定当前 `employee_id`、完整授权请求摘要和到期时间，有效期 300 秒。确认提交时，服务端重新检查 Cookie、用户、客户端、授权参数和 CSRF token；篡改隐藏字段、复用到其他授权请求或过期 token 都不能签发授权码。

## Redirect URI 与 PKCE

所有回调地址都使用结构化 URL 解析，不接受相对地址、userinfo 或 fragment。

- Web 客户端使用绝对 HTTPS URI，scheme、host、port、path 和 query 必须与登记值一致。只有 `OIDC_LOOPBACK_HTTP=true` 时，Web 客户端才可使用 HTTP loopback IP 进行本机开发。
- Desktop 客户端只接受 HTTP loopback IP，不接受 `localhost` 主机名。登记值固定 loopback IP、path 和 query，授权请求必须提供实际监听端口，并可用该端口替换登记值中的端口；实际请求地址会绑定到授权码，兑换时必须原样提交。
- 两类客户端都必须使用 PKCE S256。授权请求的 `code_challenge_method` 固定为 `S256`，Token 请求必须提供匹配的 `code_verifier`，不支持 `plain`。
- Web 客户端还必须使用 `client_secret_basic`，并在 PKCE 校验之外验证 secret。Desktop 客户端按 `none` 处理，不持有 secret，也不得发送客户端认证头。

只有客户端存在、处于启用状态且 `redirect_uri` 已验证后，授权错误才允许跳转到客户端回调地址。未知客户端、停用客户端或不可信回调地址必须在 Core 本地返回错误，不能使用请求提供的地址跳转。

## OIDC 一次性材料与短期令牌

Web client secret 和授权码都由系统安全随机源生成 32 字节随机值。数据库分别保存其 SHA-256 摘要，明文 secret 只在创建或重新生成响应中显示，授权码明文只返回到已验证的回调地址。Web secret 摘要使用常量时间比较，授权码通过摘要主键定位。

授权码有效期为 120 秒，兑换时原子删除。错误或并发兑换不能让同一授权码产生第二个成功响应。授权码、client secret、JWT、Cookie、CSRF token 和完整授权请求不得进入日志、tracing、错误响应或崩溃 dump。

ID Token 和 access token 的 `exp - iat` 固定为 300 秒，JWT 验证允许 30 秒时钟偏差。ID Token 的 audience 是客户端 ID；access token 的 audience 固定为 `<issuer>/oauth2/userinfo`。Token 响应设置 `Cache-Control: no-store` 和 `Pragma: no-cache`，UserInfo 与 secret 明文响应设置 `Cache-Control: no-store`。

## OIDC 状态复查

用户和客户端状态不是授权码或 access token 中的永久授权结果：

- 发起授权、读取确认上下文和提交确认时，服务端都重新检查客户端是否启用、用户是否存在且状态为 `active`。
- 兑换授权码时，服务端重新读取客户端和用户，检查客户端启用状态、用户启用状态、回调地址、PKCE 绑定和本次 scope 是否仍在客户端允许范围内。
- UserInfo 每次调用都校验 JWT，并重新读取客户端和用户。客户端被停用或删除、用户被禁用或删除后，已签发的 access token 不能继续读取 UserInfo。

已经交给客户端的 ID Token 无法由 Core 主动收回，客户端据此建立的本地会话也不受 Core 控制。依赖方必须结合五分钟令牌期限和自身会话策略处理这一边界。

管理端禁用用户就是完整操作，不需要额外人工清单。只有要求已经建立的依赖方会话立即失效时，才需要依赖方提供撤销接口或使用更短的会话。

## OIDC 协议限制

OIDC Provider 只提供 Authorization Code Flow、`authorization_code` grant、`RS256`、PKCE S256 和 `openid`、`profile`、`email`、`phone` 四个 scope。

- 不支持 implicit flow、hybrid flow、resource owner password credentials、client credentials 或 device authorization。
- Token 响应不签发 `refresh_token`，Token 端点不接受 refresh token grant。
- 不提供动态客户端注册、token introspection、token revocation、PAR、JAR 或 CIBA 端点。
- 不提供 front-channel logout、back-channel logout 或远程浏览器会话撤销。`POST /api/auth/logout` 只清除发起请求的浏览器 Cookie。
- 不保存历史同意、授权历史、浏览器会话或 token 记录，因此不存在按会话或按 token 查询、审计和撤销的服务端状态。

## 当前未实现的安全能力

以下能力尚未实现，不能作为现有安全承诺：

- MFA、登录限流和账号锁定。
- OIDC 审计事件、按管理员身份的操作审计。
- refresh token、token revocation、token introspection。
- JWKS 新旧密钥重叠。
- Core 多副本、数据库主备、PostgreSQL 或共享数据库。
- HSM/KMS 签名。
- `groups`、`roles`、`email_verified` 等扩展声明。

外网入口暴露登录页前，应在外网 Portal 或前置 WAF 增加登录限流，并明确仅密码认证的风险。依赖方不能把 Core 的登录态当作自身会话，应自行维护设备凭据、会话有效期和撤销。

## AD 权限

Syncer 仅接受 `ldap://<FQDN>:389`，不接受 IP、LDAP over TLS、StartTLS、URL 路径、查询、片段或用户名。它以 `NetworkService` 的主机计算机账号通过 Kerberos GSS-API 绑定 FQDN 对应的 LDAP SPN；协商保密层失败时，Syncer 以该通道失败确认，不推进 revision，且不回退到 Simple Authentication 或 NTLM。

域管理员只向 `<DOMAIN>\<SYNCER-HOST>$` 委派镜像根和隔离 OU 内的必要权限，包括创建、移动、属性写入、组成员写入、禁用和 Reset Password。不得使用 `superuser`、其他内置本地服务身份、本地账户或保存 LDAP 密码。

域控高权限凭据不得集中存放在 Core。Core 只保存业务事实、域同步配置和 Syncer 认证摘要。
