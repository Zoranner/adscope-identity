# Center OIDC 统一登录设计

## 目标与约束

Center 直接提供受限的 OpenID Connect 登录服务，让预先登记的 Web 和桌面客户端使用 Center 账号完成统一登录。Center 继续作为用户、密码和身份资料的唯一来源，OAuth 客户端由现有管理端维护。

设计遵守以下约束：

- OIDC 能力运行在现有 Center 进程中，不增加独立服务、容器、数据库或账号来源。
- 只提供统一登录和基础身份信息，不授权客户端调用 Center 业务接口。
- 每次交互式授权请求都显示确认页面，不保存历史同意结果，也不支持静默登录。
- 不增加角色、权限、客户端所有者、授权历史、审计、服务端浏览器会话或令牌记录。
- 不使用 `oxide-auth`、Hydra、Rauthy 或 Keycloak。协议编排由 Center 实现，密码学、随机数、JWT、URL、Cookie 和编码使用成熟库。
- 用户登录、个人资料、改密、管理入口和 Connector 同步的现有契约保持兼容。

## 系统边界

OIDC Provider 作为 Center 内部模块使用现有 Axum 服务、Repository 和 Nuxt 页面：

- 用户认证继续通过 `username` 查询用户，并使用现有 Argon2id verifier 校验密码。
- 用户稳定标识继续使用 `employee_id`，OIDC `sub` 不引入新的身份 ID。
- 浏览器登录状态使用无状态签名 Cookie，不建立会话表。
- OAuth 客户端和一次性授权码写入 Center 数据库。
- ID Token 和短期 access token 使用签名 JWT，不建立 token 表。
- UserInfo 实时读取现有用户表，不复制用户资料。

现有 `/api/auth/login` 在保持原 JSON 响应和 Bearer token 行为的同时设置 SSO Cookie。现有 `/api/me/*` 继续接受原 Bearer token，OIDC access token 不能用于这些接口。管理端继续使用现有管理 token，OIDC 客户端管理接口归入 `/api/admin/oauth-clients`。

## 协议范围

Center 提供以下公开端点：

- `GET /.well-known/openid-configuration`
- `GET /oauth2/authorize`：接收客户端授权请求。
- `POST /oauth2/authorize`：接收 Center 确认页的确认或取消操作。
- `POST /oauth2/token`：接收 `application/x-www-form-urlencoded` Token 请求。
- `GET /oauth2/userinfo`：接收 Bearer access token。
- `GET /oauth2/jwks`
- `POST /api/auth/logout`

授权端点只接受以下组合：

- `response_type=code`
- `scope` 包含 `openid`
- 请求提供非空 `state` 和 `nonce`
- 请求提供 `code_challenge`
- `code_challenge_method=S256`
- `client_id` 对应已启用的预登记客户端
- `redirect_uri` 符合客户端登记规则

Token 端点只接受 `grant_type=authorization_code`。Web 客户端使用 `client_secret_basic` 认证，并同时验证 PKCE；桌面客户端是 public client，不持有 secret，只验证 PKCE。两类客户端都必须在 Token 请求中提交与授权请求完全一致的 `client_id` 和 `redirect_uri`。

成功的 Token 响应包含 `access_token`、`token_type=Bearer`、`expires_in=300`、`id_token` 和实际授权的 `scope`，不包含 `refresh_token`。

Center 不支持 implicit flow、hybrid flow、resource owner password credentials、client credentials、device authorization、refresh token、dynamic client registration、token introspection、token revocation、PAR、JAR、CIBA、front-channel logout 或 back-channel logout。

## Scope 与身份声明

Center 支持固定 scope：

- `openid`：提供 `sub`，值为用户 `employee_id`。
- `profile`：提供 `preferred_username` 和 `name`。
- `email`：提供非空时的 `email`。
- `phone`：提供非空时的 `phone_number`。

客户端只能申请管理端允许的 scope。`openid` 始终必需，其他 scope 由管理员按客户端选择。确认页按实际申请的 scope 显示将提供的字段及当前值。

ID Token 包含 `iss`、`sub`、`aud`、`iat`、`exp`、`auth_time` 和授权请求中的 `nonce`，并包含已同意 scope 对应的身份声明。UserInfo 返回相同 scope 允许的当前用户资料。没有值的可选字段不写入声明，不返回空字符串。

## 客户端模型

`oauth_clients` 是客户端配置的唯一来源，保存：

- `client_id`：由 Center 生成的高熵稳定 ID，创建后不可修改。
- `name`：管理端和确认页使用的显示名称。
- `client_type`：固定为 `web` 或 `desktop`。
- `client_secret_hash`：只为 Web 客户端保存。
- `redirect_uris`：允许的回调地址列表。
- `allowed_scopes`：允许申请的 scope 列表。
- `enabled`：控制客户端能否发起和完成授权。

Web 客户端创建时生成至少 256 位随机 secret，只在创建响应中显示一次。数据库保存 secret 的 SHA-256 摘要，校验使用常量时间比较。管理端允许重新生成一个 secret，新 secret 立即替代旧 secret，不维护版本或并行有效期。创建和重新生成 secret 的响应设置 `Cache-Control: no-store`。

桌面客户端不生成 secret。Web 与桌面客户端都不能自行注册或修改配置。

管理端支持查看、创建、编辑、启用、停用和删除客户端。删除或停用客户端后不能发起授权，尚未兑换的授权码也不能完成兑换。不增加客户端所有者、角色、授权策略、操作历史或密钥轮换计划。

## Redirect URI 规则

Web 客户端的 redirect URI 必须是绝对 HTTPS URI，并完整匹配 scheme、host、port、path 和 query。只允许在明确的本机开发配置中使用 HTTP loopback URI。不接受通配域名、相对地址、userinfo、fragment 或动态拼接路径。

桌面客户端只允许系统浏览器配合 loopback IP 回调。登记配置固定 scheme、loopback IP、path 和 query，授权请求只允许端口变化。请求使用的实际 redirect URI 完整保存到授权码中，Token 请求必须提交同一个 URI。`localhost` 主机名不作为 loopback IP 的替代，避免 DNS 和地址族解析差异。

URI 解析、规范化和比较必须使用结构化 URL API，不能使用字符串前缀、后缀或包含关系判断。错误 URI 不得作为错误响应跳转目标。

## 授权流程

客户端通过系统浏览器或普通浏览器导航到 `/oauth2/authorize`。Center 在任何页面跳转前校验客户端、redirect URI、response type、scope、state、nonce 和 PKCE 参数。

浏览器没有有效 SSO Cookie 时，Center 跳转到现有登录页。登录成功后设置 SSO Cookie，并返回服务端重建的原授权请求。登录返回目标只允许 Center 内部授权地址，不能接受任意外部 `return_to`。

浏览器已有有效 SSO Cookie 时，Center 直接进入确认页。确认页显示：

- 客户端名称。
- 当前登录用户。
- 本次将提供的身份字段和值。
- 确认登录、取消和切换账号操作。

确认页不提供记住选择或自动同意。切换账号先清除当前 SSO Cookie，再回到同一授权请求。确认提交包含绑定当前登录状态和当前授权请求的短期 CSRF token。

服务端收到确认或取消操作时重新校验全部授权参数、客户端状态、用户状态、SSO Cookie 和 CSRF token，不能信任页面隐藏字段。用户确认后生成一次性授权码，并向已验证的 redirect URI追加 `code` 和原始 `state`。用户取消时返回 `error=access_denied` 和原始 `state`。

`prompt=none` 不会绕过确认页面。Center 对它返回 `error=interaction_required`，不签发授权码。

## 授权码

`oauth_authorization_codes` 保存：

- 授权码 SHA-256 摘要。
- `client_id`。
- 用户 `employee_id`。
- 授权请求使用的实际 `redirect_uri`。
- 用户实际同意的 scope。
- `nonce`。
- PKCE `code_challenge`。
- 登录发生时间 `auth_time`。
- 到期时间。

授权码明文使用至少 256 位安全随机数，只通过 redirect URI 返回。授权码有效期为两分钟。

Token 端点按授权码摘要定位记录，并在同一数据库事务内校验客户端、redirect URI、PKCE、到期时间、客户端状态和用户状态。授权码一经提交兑换即被原子删除；任何一个授权码最多只能产生一次成功响应，并发兑换只能有一个请求成功。失败兑换不返回记录中的任何绑定信息。

数据库不保存已使用状态、兑换历史或授权历史。到期记录在创建和兑换授权码的现有请求路径中进行有限批量清理，不增加周期任务或独立服务。

## 浏览器登录状态

SSO Cookie 是 Center 交互式登录所需的无状态浏览器凭据，包含用户 `employee_id`、登录时间和到期时间，并使用标准 HMAC-SHA256 签名。现有普通 SHA-256 拼接密钥的会话签名实现需要由标准 HMAC 替代。

Cookie 使用以下属性：

- `HttpOnly`
- `Secure`
- `SameSite=Lax`
- `Path=/`

Cookie 默认有效期继续使用现有一小时配置。`SameSite=Lax` 允许客户端以浏览器顶层导航发起授权，同时阻止跨站 POST 携带登录状态。确认操作仍需要 CSRF token，不能只依赖 SameSite。

`/api/auth/logout` 清除当前浏览器的 SSO Cookie。由于不建立服务端会话表，Center 不支持远程撤销单个浏览器会话，也不主动退出其他客户端已经建立的本地会话。

禁用用户后，新的授权、授权码兑换和 UserInfo 立即失败。已经签发的 ID Token 最多继续有效五分钟，这是无状态短期令牌设计的明确边界。

## JWT 与 JWKS

Center 使用一把稳定的 RSA 私钥和 `RS256` 签发 ID Token 与 access token。私钥通过受限环境配置或挂载文件提供，不写入数据库、源码、日志、镜像或发布包。JWKS 端点只发布对应公钥，`kid` 根据公钥稳定生成。

Center 使用显式配置的外部 HTTPS issuer，例如：

```text
ADSS_OIDC_ISSUER=https://center.example.com
```

Discovery 地址、端点地址和 JWT `iss` 都由该配置生成，不能根据请求的 `Host`、`Forwarded` 或 `X-Forwarded-*` 动态推导。启动时必须校验 issuer 和私钥配置；配置无效时明确拒绝启动，不能临时生成会在重启后变化的签名密钥。

ID Token 和 access token 有效期均为五分钟。access token 包含 `iss`、`sub`、`aud`、`client_id`、`scope`、`iat` 和 `exp`，audience 固定指向 Center UserInfo。它只能用于 `/oauth2/userinfo`，不能通过现有普通用户 Bearer token 校验路径访问 Center 业务接口。

管理端不提供 OIDC 签名密钥管理和轮换页面。部署人员手工更换私钥并重启后，旧令牌最多继续存在五分钟；JWKS 只发布当前公钥，不维护历史密钥集合。

## 错误处理

Center 只在 `client_id` 存在、客户端启用且 redirect URI 已确认可信后，才允许向该 URI 返回 OAuth 错误。未知客户端、停用客户端或非法 redirect URI 必须显示 Center 本地错误页面，绝不能跳转到请求提供的地址。

redirect URI 已验证后，授权请求错误使用标准 OAuth 错误参数和原始 `state` 返回。包括：

- 无效或缺少参数返回 `invalid_request`。
- 不支持的 response type 返回 `unsupported_response_type`。
- 不允许的 scope 返回 `invalid_scope`。
- `prompt=none` 返回 `interaction_required`。
- 用户取消返回 `access_denied`。

Token 端点使用标准 JSON 错误响应，不执行浏览器跳转。授权码未知、过期、已使用、绑定信息不匹配或 PKCE 校验失败返回 `invalid_grant`；Web 客户端认证失败返回 `invalid_client`；不支持的 grant type 返回 `unsupported_grant_type`。

登录失败统一返回账号或密码错误，不暴露用户名是否存在。数据库、签名或内部状态故障不向客户端暴露 SQL、文件路径、密钥、凭据、堆栈或内部错误文本。

## 安全规则

- OIDC 公开入口必须位于 TLS 后，生产 issuer 必须使用 HTTPS。
- 授权码、client secret、JWT、Cookie、密码、CSRF token 和完整授权请求不得进入应用日志。
- Token、UserInfo、secret 创建和 secret 重新生成响应设置 `Cache-Control: no-store`；Token 响应同时设置 `Pragma: no-cache`。
- UserInfo 每次校验 JWT 签名、issuer、audience、有效期、client 状态、用户状态和 scope。
- JWT 时间校验只允许有限时钟偏差，不能通过宽松偏差延长五分钟有效期。
- 参数数量、单项长度、URI 长度、scope 数量和请求体大小设置明确上限。
- Token 端点不启用宽泛 CORS。Web 客户端从自己的后端兑换授权码，桌面客户端直接调用 Token 端点。
- OAuth 参数和回调地址使用结构化解析，JSON、表单和 Header 解析失败统一进入受控错误路径。

## Discovery

Discovery 只声明 Center 实际支持的能力：

- `response_types_supported` 只包含 `code`。
- `grant_types_supported` 只包含 `authorization_code`。
- `code_challenge_methods_supported` 只包含 `S256`。
- `subject_types_supported` 只包含 `public`。
- `id_token_signing_alg_values_supported` 只包含 `RS256`。
- scope 只包含 `openid`、`profile`、`email` 和 `phone`。
- Token 端点认证方式包含 `client_secret_basic` 和 `none`，分别用于 Web 与桌面客户端。

未实现的动态注册、刷新、撤销、introspection 和联合注销端点不写入 Discovery。

## 兼容性

数据库初始化以增量方式创建 OAuth 表，不修改现有用户、凭据、组织、域和同步数据。Repository 的 SQLite 和 PostgreSQL 实现必须保持相同的客户端查询和授权码原子消费语义。

现有 Center 页面继续使用普通用户 Bearer token。SSO Cookie 只用于浏览器授权流程，不替代管理 token，不参与 Connector 认证，也不扩大普通用户接口权限。

桌面客户端必须使用系统浏览器，不允许嵌入 WebView 收集 Center 密码。桌面应用在本机临时监听登记过的 loopback IP 和 path，使用随机端口接收授权结果。

## 验证标准

自动化验证覆盖：

- Discovery、JWKS、JWT 算法和端点声明相互一致。
- Web confidential client 和 Desktop public client 的完整授权链路。
- PKCE `S256` 标准测试向量。
- Web redirect URI 精确匹配和桌面 loopback 端口匹配。
- scope 对确认页、ID Token 和 UserInfo 字段的约束。
- ID Token 可以通过 JWKS 公钥验证，且 `iss`、`aud`、`nonce` 和时间字段正确。
- 授权码过期、重复兑换、并发兑换、客户端不匹配、redirect URI 不匹配和 PKCE 错误。
- 用户取消、`prompt=none`、未知客户端、停用用户和停用客户端。
- 错误请求不会向未验证的地址跳转。
- Cookie 签名、到期、CSRF、切换账号和退出登录行为。
- Token 和错误响应不包含密码、secret 或内部异常。
- 现有登录、自助资料、改密、管理端和 Connector 契约不回归。

Rust 代码修改后执行 workspace 格式化、全目标全 feature Clippy 和相关测试。根 workspace 排除的 `crates/protocol` 与 `crates/store` 继续通过各自 manifest 单独验证。Nuxt 使用 Bun 执行现有类型检查和构建，不添加 Playwright，不启动浏览器自动化。

在正式声明兼容通用 OIDC 客户端前，还必须使用打包后的 Center 分别完成真实 Web 客户端和桌面 loopback 客户端接入，并运行 OpenID Foundation conformance suite。缺少外部测试条件时必须明确记录未验证边界，不能以单元测试代替协议一致性和真实浏览器交互验收。
