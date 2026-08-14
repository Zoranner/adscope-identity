# 安全要求

## 密码事实源

用户密码通过中心服务修改，并由各域 Connector 同步到本地域控。域内 AD 的普通 Change Password 路径不作为事实源，也不会传播到其他域。

受管账号应禁止用户在 AD 中自行 Change Password，并通过 GPO 隐藏 `Ctrl+Alt+Del` 的“更改密码”入口。Connector 使用被委派的 Reset Password 权限设置中心下发的密码。

## 密钥和配置

主服务必须配置高熵 `PASSWORD_ENCRYPTION_KEY`、`SESSION_KEY` 和 `MANAGEMENT_TOKEN`。这些密钥通过受限 `.env`、系统环境变量、Windows DPAPI、Secret Manager 或等价机制注入，不进入源码、日志或配置仓库。

OIDC 使用独立 RSA 私钥签发 RS256 ID token 和 access token。私钥至少为 2048 位，通过受限文件提供给 Center，不写入仓库、`.env`、环境变量、容器镜像或日志。OIDC 私钥只用于 token 签名，不是反向代理使用的 TLS 证书。

Connector key 按域独立保存，是 Connector `.env` 中唯一的同步秘密；运行时不得写入日志、错误响应或配置仓库。Connector 不保存 LDAP bind DN 或 LDAP 密码。

## SSO 会话

用户登录成功后，Center 设置 `adscope_sso` Cookie，用于浏览器内的 OIDC 授权流程。Cookie 使用 `HttpOnly`、`Secure`、`SameSite=Lax` 和 `Path=/`，内容由 `SESSION_KEY` 签名。SSO Cookie 不替代普通用户 Bearer token、管理 token 或 Connector key，也不能用于 `/api/me/*`、`/api/admin/*` 或 Connector 接口。

授权确认提交使用短期 CSRF token，并绑定登录用户和完整授权请求。`SameSite=Lax` 不能替代这项校验。

Center 不签发 refresh token，也没有远程撤销单个浏览器 SSO 会话或已签发 OIDC token 的机制。退出登录只清除当前浏览器的 SSO Cookie，不会退出其他浏览器，也不会撤销接入系统已经建立的本地会话。OIDC ID token 和 access token 的有效期固定为 5 分钟，到期后由接入系统重新发起 OIDC 授权。

## 传输要求

主服务必须位于 TLS 后面。Connector 调用 `/api/connector/sync` 获取凭据材料时，响应不能经过明文 HTTP，代理、网关、日志、tracing、错误回显和崩溃 dump 不记录响应体。

Connector 仅通过 `ldap://<FQDN>:389` 访问域控，并以 `NetworkService` 下的主机计算机账号发起 Kerberos GSS-API 认证。域控 FQDN 必须对应 LDAP SPN；GSS-API 协商的保密层失败即停止该批次，不回退到 Simple Authentication、NTLM、LDAP over TLS 或 StartTLS。

## 日志和诊断

应用、反向代理、网关、tracing、错误回显和崩溃 dump 不得记录普通用户 token、OIDC access token、ID token、授权码、客户端 secret、SSO Cookie、CSRF token 或完整授权请求。排障日志只保留必要的请求标识、端点、结果状态和不含凭据的错误分类。

## 权限边界

普通用户 token 只用于本人资料和本人改密接口。普通用户不能选择其他用户，也不能修改工号、账号名、显示名、组织归属、启用状态、组成员或域配置。

管理入口必须使用独立保护，不能复用普通用户 token。域、OU、用户、组、管理员代设密码和同步状态查询接口都属于管理面。

域管理员只向 `<DOMAIN>\<CONNECTOR-HOST>$` 委派镜像根和隔离 OU 内的必要权限，不使用 `superuser`、其他内置本地服务身份或本地账户。权限包括创建、移动、属性写入、组成员写入、禁用和 Reset Password，且不得扩展到委派范围外。
