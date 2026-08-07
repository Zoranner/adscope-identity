# 安全实现边界

## 密码事实源

用户密码只能通过中心服务修改。域内 AD 的普通 Change Password 路径不作为事实源，也不会传播到其他域。

受管账号应禁止用户在 AD 中自行 Change Password，并通过 GPO 隐藏 `Ctrl+Alt+Del` 的“更改密码”入口。Connector 使用被委派的 Reset Password 权限设置中心下发的密码。

## Verifier 与 Ciphertext

`user_credentials.password_verifier` 用于中心登录和改密前校验。生产配置使用 Argon2id PHC 字符串；verifier 不能还原密码，也不下发给 Connector。

`user_credentials.password_ciphertext` 用于保存中心当前密码材料。主服务通过密码加密方式处理密文，只有在响应 `/api/connector/sync` 时才在内存中解封，并组装 `CredentialEntry.plaintext_password`。

主服务使用内置 XChaCha20-Poly1305 加密密码材料，并通过本机受保护配置提供高熵 `ADSS_PASSWORD_ENCRYPTION_KEY`。该密钥必须和数据库备份分离保存，不能进入源码、日志或配置仓库。

## 凭据传输

凭据响应包含 Connector 可执行的明文密码，必须满足以下要求：

- 只允许通过主服务 TLS 调用 `/api/connector/sync`。
- 成功响应设置 `Cache-Control: no-store`。
- 代理、网关、日志、tracing、错误回显和崩溃 dump 不记录响应体。
- Connector 不把明文密码写入本地 state、日志或配置。
- Connector 设置密码后立即丢弃明文。

自动化测试只能验证协议、编码、映射和错误处理，不能替代 TLS、日志链路、代理缓存和真实域控权限验收。

## Connector Key

`domains.connector_key_hash` 保存 Connector key 的 `sha256:` 摘要，不保存明文 key。请求时服务端对 `x-adss-connector-key` 做同样摘要，并使用常量时间比较。

Connector key 是域绑定共享密钥。部署时应通过 Windows DPAPI、Secret Manager、受限配置文件或等价机制保存 Connector key。Connector key 不得进入日志、错误响应或配置仓库。

## 普通用户会话

普通用户登录成功后，主服务返回 HMAC-SHA256 签名的 Bearer token，并把同一个 token 写入 `adss_sso` Cookie。Bearer token 只用于 `/api/me/*` 自助接口；Cookie 只用于浏览器 OIDC 授权流程。两种入口都从签名内容中确定当前 `employee_id`，不建立服务端会话表。

`adss_sso` Cookie 使用 `HttpOnly`、`Secure`、`SameSite=Lax` 和 `Path=/`。默认有效期为一小时，可通过服务端会话 TTL 配置调整。`POST /api/auth/logout` 只清除当前浏览器 Cookie，不接受或撤销远程会话。

普通用户不能通过路径参数选择其他用户，不能修改 `employee_id`、`username`、显示名、组织归属、启用状态、组成员或域配置。

`/api/me/password` 必须根据 token 中的 `employee_id` 修改当前用户自己的密码。请求必须提供当前密码，服务端用 `password_verifier` 校验后才写入新的 verifier 和 ciphertext。

`ADSS_USER_SESSION_KEY` 和 `ADSS_MANAGEMENT_TOKEN` 是生产密钥材料，必须通过受限 `.env`、系统环境变量、Secret Manager、Windows DPAPI 或等价机制注入，不得进入源码、日志或配置仓库。

## 管理入口

管理入口必须位于受保护入口后，且必须和普通用户自助 token 区分。

域、OU、用户、组、管理员代设密码和同步状态查询接口都属于管理面。管理员代设密码不要求用户当前密码，因此不能复用普通用户自助改密的安全语义。

`/api/admin/*` 不得在缺少管理入口保护的情况下暴露给公网或普通用户入口。

## OIDC 身份与 issuer

Center OIDC 复用现有用户名和密码校验，不复制账号或身份资料。OIDC `sub` 使用用户 `employee_id`；`profile`、`email` 和 `phone` 声明从用户表读取。OIDC access token 只授权访问 `/oauth2/userinfo`，不能代替普通用户 Bearer token、管理 token 或 Connector key。

`ADSS_OIDC_ISSUER` 必须是仅包含 HTTPS scheme、host 和可选端口的外部 origin，不接受 userinfo、业务路径、query 或 fragment。Discovery 地址、JWT `iss` 和各公开端点都从该配置生成，不能根据 `Host`、`Forwarded` 或 `X-Forwarded-*` 动态推导。

签名私钥通过 `ADSS_OIDC_PRIVATE_KEY_FILE` 指向的受限文件提供。密钥必须是至少 2048 位的 RSA PKCS#8 或 PKCS#1 PEM，不能写入数据库、源码、日志、镜像或发布包。Center 使用 `RS256` 签发 ID Token 和 access token，JWKS 只发布当前公钥；服务端不保存历史签名密钥集合，也不提供管理端密钥轮换接口。

## OIDC 授权确认

浏览器已有有效 SSO Cookie 时仍要进入确认页。服务端不保存历史同意结果，`prompt=none` 固定返回 `interaction_required`，不能用于静默签发授权码。

确认页通过 `/api/oauth2/authorize/context` 取得短期 CSRF token。该 token 使用 HMAC-SHA256 签名，绑定当前 `employee_id`、完整授权请求摘要和到期时间，有效期 300 秒。确认提交时，服务端重新检查 Cookie、用户、客户端、授权参数和 CSRF token；篡改隐藏字段、复用到其他授权请求或过期 token 都不能签发授权码。

## Redirect URI 与 PKCE

所有回调地址都使用结构化 URL 解析，不接受相对地址、userinfo 或 fragment。

- Web 客户端使用绝对 HTTPS URI，scheme、host、port、path 和 query 必须与登记值一致。只有 `ADSS_OIDC_ALLOW_INSECURE_WEB_LOOPBACK_REDIRECTS=true` 时，Web 客户端才可使用 HTTP loopback IP 进行本机开发。
- Desktop 客户端只接受 HTTP loopback IP，不接受 `localhost` 主机名。登记值固定 loopback IP、path 和 query，授权请求必须提供实际监听端口，并可用该端口替换登记值中的端口；实际请求地址会绑定到授权码，兑换时必须原样提交。
- 两类客户端都必须使用 PKCE S256。授权请求的 `code_challenge_method` 固定为 `S256`，Token 请求必须提供匹配的 `code_verifier`，不支持 `plain`。
- Web 客户端还必须使用 `client_secret_basic`，并在 PKCE 校验之外验证 secret。Desktop 客户端按 `none` 处理，不持有 secret，也不得发送客户端认证头。

只有客户端存在、处于启用状态且 `redirect_uri` 已验证后，授权错误才允许跳转到客户端回调地址。未知客户端、停用客户端或不可信回调地址必须在 Center 本地返回错误，不能使用请求提供的地址跳转。

## OIDC 一次性材料与短期令牌

Web client secret 和授权码都由系统安全随机源生成 32 字节随机值。数据库分别保存其 SHA-256 摘要，明文 secret 只在创建或重新生成响应中显示，授权码明文只返回到已验证的回调地址。Web secret 摘要使用常量时间比较，授权码通过摘要主键定位。

授权码有效期为 120 秒，兑换时原子删除。错误或并发兑换不能让同一授权码产生第二个成功响应。授权码、client secret、JWT、Cookie、CSRF token 和完整授权请求不得进入日志、tracing、错误响应或崩溃 dump。

ID Token 和 access token 的 `exp - iat` 固定为 300 秒，JWT 验证允许 30 秒时钟偏差。ID Token 的 audience 是客户端 ID；access token 的 audience 固定为 `<issuer>/oauth2/userinfo`。Token 响应设置 `Cache-Control: no-store` 和 `Pragma: no-cache`，UserInfo 与 secret 明文响应设置 `Cache-Control: no-store`。

## OIDC 状态复查

用户和客户端状态不是授权码或 access token 中的永久授权结果：

- 发起授权、读取确认上下文和提交确认时，服务端都重新检查客户端是否启用、用户是否存在且状态为 `active`。
- 兑换授权码时，服务端重新读取客户端和用户，检查客户端启用状态、用户启用状态、回调地址、PKCE 绑定和本次 scope 是否仍在客户端允许范围内。
- UserInfo 每次调用都校验 JWT，并重新读取客户端和用户。客户端被停用或删除、用户被禁用或删除后，已签发的 access token 不能继续读取 UserInfo。

已经交给客户端的 ID Token 无法由 Center 主动收回，客户端据此建立的本地会话也不受 Center 控制。依赖方必须结合五分钟令牌期限和自身会话策略处理这一边界。

## OIDC 协议限制

OIDC Provider 只提供 Authorization Code Flow、`authorization_code` grant、`RS256`、PKCE S256 和 `openid`、`profile`、`email`、`phone` 四个 scope。

- 不支持 implicit flow、hybrid flow、resource owner password credentials、client credentials 或 device authorization。
- Token 响应不签发 `refresh_token`，Token 端点不接受 refresh token grant。
- 不提供动态客户端注册、token introspection、token revocation、PAR、JAR 或 CIBA 端点。
- 不提供 front-channel logout、back-channel logout 或远程浏览器会话撤销。`POST /api/auth/logout` 只清除发起请求的浏览器 Cookie。
- 不保存历史同意、授权历史、浏览器会话或 token 记录，因此不存在按会话或按 token 查询、审计和撤销的服务端状态。

## AD 权限

Connector 仅接受 `ldap://<FQDN>:389`，不接受 IP、LDAP over TLS、StartTLS、URL 路径、查询、片段或用户名。它以 `NetworkService` 的主机计算机账号通过 Kerberos GSS-API 绑定 FQDN 对应的 LDAP SPN；协商保密层失败时，Connector 以该通道失败确认，不推进 revision，且不回退到 Simple Authentication 或 NTLM。

域管理员只向 `<DOMAIN>\<CONNECTOR-HOST>$` 委派镜像根和隔离 OU 内的必要权限，包括创建、移动、属性写入、组成员写入、禁用和 Reset Password。不得使用 `superuser`、其他内置本地服务身份、本地账户或保存 LDAP 密码。

域控高权限凭据不得集中存放在主服务。主服务只保存中心业务事实、域同步配置和 Connector 认证摘要。
