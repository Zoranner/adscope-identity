# 安全要求

## 密码事实源

用户密码通过中心服务修改，并由各域 Connector 同步到本地域控。域内 AD 的普通 Change Password 路径不作为事实源，也不会传播到其他域。

受管账号应禁止用户在 AD 中自行 Change Password，并通过 GPO 隐藏 `Ctrl+Alt+Del` 的“更改密码”入口。Connector 使用被委派的 Reset Password 权限设置中心下发的密码。

## 密钥和配置

主服务必须配置高熵 `PASSWORD_ENCRYPTION_KEY`、`SESSION_KEY` 和 `MANAGEMENT_TOKEN`。这些密钥通过受限 `.env`、系统环境变量、Windows DPAPI、Secret Manager 或等价机制注入，不进入源码、日志或配置仓库。

OIDC 使用独立 RSA 私钥签发 RS256 ID token 和 access token。私钥至少为 2048 位，通过受限文件提供给 Center，不写入仓库、`.env`、环境变量、容器镜像或日志。OIDC 私钥只用于 token 签名，不是网关使用的 TLS 证书。

Connector key 按域独立保存，是 Connector `.env` 中唯一的同步秘密；运行时不得写入日志、错误响应或配置仓库。Connector 不保存 LDAP bind DN 或 LDAP 密码。

## SSO 会话

用户登录成功后，Center 设置 `adscope_sso` Cookie，用于浏览器内的 OIDC 授权流程。Cookie 使用 `HttpOnly`、`Secure`、`SameSite=Lax` 和 `Path=/`，内容由 `SESSION_KEY` 签名。SSO Cookie 不替代普通用户 Bearer token、管理会话 Cookie 或 Connector key，也不能用于 `/api/me/*`、`/api/admin/*` 或 Connector 接口。

授权确认提交使用短期 CSRF token，并绑定登录用户和完整授权请求。`SameSite=Lax` 不能替代这项校验。

Center 不签发 refresh token，也没有远程撤销单个浏览器 SSO 会话或已签发 OIDC token 的机制。退出登录只清除当前浏览器的 SSO Cookie，不会退出其他浏览器，也不会撤销接入系统已经建立的本地会话。OIDC ID token 和 access token 的有效期固定为 5 分钟，到期后由接入系统重新发起 OIDC 授权。

## 当前安全边界

Center 当前是单实例 SQLite 服务，以下能力尚未实现：

- 只有用户名和密码登录，没有 MFA。
- 登录和授权接口没有应用级限流或账号锁定；外网入口应在外网网关或前置 WAF 增加登录限流。
- 没有 OIDC 审计事件，也没有按管理员身份的操作审计。
- 没有 refresh token、token revocation、introspection 或远程会话撤销。禁用用户会阻止新的授权、授权码兑换和 UserInfo，但已签发 ID Token 最多继续有效 5 分钟，浏览器 SSO Cookie 默认最多 1 小时。
- 只有一把活动 RSA 签名密钥，JWKS 不保留旧 key；更换私钥需要维护窗口。
- 生产入口固定 SQLite 单文件，不支持多个 Center 副本共享写，也没有数据库主备。

依赖方不能把 Center 的登录态当作自身会话。依赖方需要长期会话或设备凭据时，应自行定义有效期、重新验证和撤销策略。

管理端禁用用户就是完整操作，不需要额外人工清单。只有要求已经建立的依赖方会话立即失效时，才需要依赖方提供撤销接口或使用更短的会话。

## OIDC 网络入口

Center 只使用一个 canonical issuer；三个网关的入口域名和证书可以不同。网关负责 TLS、静态前端、API/OIDC 转发和两组入口路径判断，不保存用户、Token 或签名私钥，也不签发本地 Token。

网关终止 TLS 时会看到密码、Cookie、client secret 和 Token 响应，必须禁止记录或缓存这些内容。Center 本身不管理 TLS 证书。

三个入口域名可以重定向到 canonical issuer，但 OIDC 客户端应直接使用 canonical issuer 做 discovery、authorize 和校验 `iss`。当前 `adscope_sso` Cookie 没有 `Domain` 属性，用不同入口域名登录会产生独立登录态。

每个网关通过 `LOGIN_ENABLED` 和 `ADMIN_ENABLED` 两个环境变量控制登录和管理。网关命中禁用路由时直接返回 `403` 错误页，不转发给 Center；Center 不参与开关判断。关闭登录不撤销已签发 Token，也不影响依赖方本地会话；关闭管理不影响用户登录和 Connector 同步。内网入口 A、内网入口 B网关可以开启管理，外网网关关闭管理。网关必须能按这两组路径判断；只能整站开关的实现无法独立控制登录和管理。

## 传输要求

主服务必须位于 TLS 后面。Connector 调用 `/api/connector/sync` 获取凭据材料时，响应不能经过明文 HTTP，网关、日志、tracing、错误回显和崩溃 dump 不记录响应体。

Connector 仅通过 `ldap://<FQDN>:389` 访问域控，并以 `NetworkService` 下的主机计算机账号发起 Kerberos GSS-API 认证。域控 FQDN 必须对应 LDAP SPN；GSS-API 协商的保密层失败即停止该批次，不回退到 Simple Authentication、NTLM、LDAP over TLS 或 StartTLS。

## 日志和诊断

应用、网关、tracing、错误回显和崩溃 dump 不得记录普通用户 token、OIDC access token、ID token、授权码、客户端 secret、SSO Cookie、CSRF token 或完整授权请求。排障日志只保留必要的请求标识、端点、结果状态和不含凭据的错误分类。

## 权限边界

普通用户 token 只用于本人资料和本人改密接口。普通用户不能选择其他用户，也不能修改工号、账号名、显示名、组织归属、启用状态、组成员或域配置。

管理入口必须使用独立保护，不能复用普通用户 token。域、OU、用户、组、管理员代设密码和同步状态查询接口都属于管理面。

内网管理可以复用内网网关，但外网入口不得暴露管理面。Center 只校验 `MANAGEMENT_TOKEN`，不判断请求来源网络；来源限制由网关、防火墙、VLAN 或 VPN 负责。

域管理员只向 `<DOMAIN>\<CONNECTOR-HOST>$` 委派镜像根和隔离 OU 内的必要权限，不使用 `superuser`、其他内置本地服务身份或本地账户。权限包括创建、移动、属性写入、组成员写入、禁用和 Reset Password，且不得扩展到委派范围外。
