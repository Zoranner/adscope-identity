# API 契约

## 设计原则

API 按调用身份分组：

- `/api/auth/*` 用于普通用户登录。
- `/api/me/*` 用于普通用户自助，只能操作 token 对应的本人账号。
- `/.well-known/openid-configuration` 和 `/oauth2/*` 用于预登记客户端发起 OIDC 登录。
- `/api/oauth2/authorize/context` 只向持有有效 SSO Cookie 的确认页返回本次授权上下文。
- `/api/admin/*` 用于受保护管理入口，写入中心目录和域配置事实。
- `/api/connector/*` 用于域内 Connector 同步，必须通过域绑定的 Connector key 鉴权。

中心数据库是 API 写入的唯一事实源。普通用户、管理员和 Connector 都不直接写 AD；AD 写入只由域内 Connector 通过同步协议执行。

管理入口必须和普通用户自助入口区分身份边界。系统使用受保护管理凭证区分管理调用，不引入管理员账号、角色或审批流程。

## 通用约定

请求和响应使用 JSON。服务端错误使用标准 HTTP 状态码：

| 状态码 | 含义 |
| --- | --- |
| `400 Bad Request` | 请求结构或字段值非法。 |
| `401 Unauthorized` | 缺少凭证、凭证错误或登录失败。 |
| `403 Forbidden` | 已认证但无权访问目标资源。 |
| `404 Not Found` | 目标对象不存在。 |
| `409 Conflict` | 请求与唯一约束、revision 或状态规则冲突。 |
| `500 Internal Server Error` | 服务端持久化、密码加密或外部依赖错误。 |

密码明文、密码密文和 Connector key 明文不得出现在普通查询响应、错误响应或日志中。

## 普通用户接口

### 用户登录

`POST /api/auth/login`

请求：

```json
{
  "username": "zhangsan",
  "password": "CurrentPass123!"
}
```

行为：

- 根据 `users.username` 定位用户。用户名必须唯一，不能用工号代替登录名。
- 使用 `user_credentials.password_verifier` 验证用户提交的密码。
- 登录成功后签发普通用户自助接口使用的 Bearer token。
- 登录响应同时把同一个 token 写入 `adscope_sso` Cookie，供浏览器 OIDC 授权流程识别登录状态。Cookie 使用 `HttpOnly`、`Secure`、`SameSite=Lax` 和 `Path=/`。
- 登录失败返回 `401 Unauthorized`。

响应：

```json
{
  "employee_id": "1001",
  "access_token": "adscope-user-session:v2.<payload>.<signature>"
}
```

### 退出浏览器登录

`POST /api/auth/logout`

服务端返回 `204 No Content`，并通过 `Max-Age=0` 的 `adscope_sso` Cookie 清除当前浏览器登录状态。该接口不接受会话 ID，也不撤销其他浏览器中的无状态登录凭证。

### 本人资料

以下接口必须携带普通用户 token：

```text
Authorization: Bearer <access_token>
```

`GET /api/me`

行为：

- 根据 token 读取当前用户自己的目录资料。
- 不接受 employee_id、username 或其他用户选择参数。
- 缺少 token、token 过期或签名错误时返回 `401 Unauthorized`。

响应：

```json
{
  "employee_id": "1001",
  "username": "zhangsan",
  "display_name": "张三",
  "email": "zhangsan@example.com",
  "mobile": "13800000000",
  "telephone": "021-10000000",
  "organizational_unit_id": "ou-rd",
  "status": "active"
}
```

### 本人联系方式

`PATCH /api/me/contact`

请求：

```json
{
  "email": "zhangsan@example.com",
  "mobile": "13800000000",
  "telephone": "021-10000000"
}
```

行为：

- 只允许修改 token 对应用户的 `email`、`mobile` 和 `telephone`。
- 不接受 `employee_id`、`username`、`display_name`、`organizational_unit_id`、`status` 或组成员字段。
- 成功后推进目录 revision，由 Connector 同步到各域。

响应：

```json
{
  "profile": {
    "employee_id": "1001",
    "username": "zhangsan",
    "display_name": "张三",
    "email": "zhangsan@example.com",
    "mobile": "13800000000",
    "telephone": "021-10000000",
    "organizational_unit_id": "ou-rd",
    "status": "active"
  },
  "directory_revision": 12
}
```

### 本人改密

`POST /api/me/password`

请求：

```json
{
  "current_password": "CurrentPass123!",
  "new_password": "NewPass123!"
}
```

行为：

- 根据 token 确定当前用户。
- 先校验当前密码，再写入新的 verifier 和 ciphertext。
- 成功后推进凭据 revision。

响应：

```json
{
  "employee_id": "1001",
  "credential_revision": 8
}
```

## OIDC 统一登录

Center 只接受预登记客户端使用 Authorization Code Flow 登录。OIDC `sub` 直接使用用户 `employee_id`，OIDC access token 只用于 UserInfo，不能用于 `/api/me/*`。

### Discovery

`GET /.well-known/openid-configuration`

响应由配置的外部 issuer 生成，不根据请求 `Host` 或代理头推导：

```json
{
  "issuer": "https://center.example.com",
  "authorization_endpoint": "https://center.example.com/oauth2/authorize",
  "token_endpoint": "https://center.example.com/oauth2/token",
  "userinfo_endpoint": "https://center.example.com/oauth2/userinfo",
  "jwks_uri": "https://center.example.com/oauth2/jwks",
  "response_types_supported": ["code"],
  "grant_types_supported": ["authorization_code"],
  "subject_types_supported": ["public"],
  "id_token_signing_alg_values_supported": ["RS256"],
  "scopes_supported": ["openid", "profile", "email", "phone"],
  "token_endpoint_auth_methods_supported": ["client_secret_basic", "none"],
  "code_challenge_methods_supported": ["S256"]
}
```

响应包含 `Cache-Control: public, max-age=300`。

### JWKS

`GET /oauth2/jwks`

响应只发布签发令牌所用 RSA 公钥：

```json
{
  "keys": [
    {
      "kty": "RSA",
      "use": "sig",
      "alg": "RS256",
      "kid": "sha256:...",
      "n": "...",
      "e": "AQAB"
    }
  ]
}
```

响应不包含私钥参数，并包含 `Cache-Control: public, max-age=300`。

### 发起授权

`GET /oauth2/authorize`

查询参数：

| 字段 | 约束 |
| --- | --- |
| `response_type` | 固定为 `code`。 |
| `client_id` | 已登记且启用的客户端 ID。 |
| `redirect_uri` | 必须符合该客户端登记的回调规则。 |
| `scope` | 空格分隔，必须包含 `openid`，可选 `profile`、`email`、`phone`，且不得超出客户端 `allowed_scopes`。 |
| `state` | 必填，长度为 1 至 512 个字符。 |
| `nonce` | 必填，长度为 1 至 256 个字符。 |
| `code_challenge` | 必填，43 个 base64url 字符。 |
| `code_challenge_method` | 固定为 `S256`。 |
| `response_mode` | 可省略；提供时只能为 `query`。 |
| `prompt` | 可省略；`none` 不跳过确认，返回 `interaction_required`。 |

参数缺失、重复、未定义、编码非法或总查询长度超过 16 KiB 时，服务端拒绝请求。

服务端先验证客户端和 `redirect_uri`。浏览器没有有效 `adscope_sso` Cookie 时，返回 `303 See Other` 到 `/login?continue=...`；已登录且用户状态为 `active` 时，返回 `303 See Other` 到 Center 的 `/authorize` 确认页。`continue` 和确认页地址都由服务端根据已验证参数重建，不接受外部返回地址。所有授权跳转包含 `Cache-Control: no-store`。

### 授权确认上下文

`GET /api/oauth2/authorize/context`

查询参数与 `GET /oauth2/authorize` 相同。请求必须携带有效 `adscope_sso` Cookie，服务端再次检查客户端、用户和全部授权参数。

响应包含本次确认所需的客户端、用户、声明预览、授权参数和短期 CSRF token：

```json
{
  "client_name": "业务门户",
  "user": {
    "employee_id": "1001",
    "username": "zhangsan",
    "display_name": "张三"
  },
  "claims": {
    "sub": "1001",
    "preferred_username": "zhangsan",
    "name": "张三",
    "email": "zhangsan@example.com"
  },
  "csrf_token": "adscope-csrf:v1....",
  "authorization": {
    "response_type": "code",
    "client_id": "client_...",
    "redirect_uri": "https://client.example.com/callback",
    "scope": "openid profile email",
    "state": "random-state",
    "nonce": "random-nonce",
    "code_challenge": "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
    "code_challenge_method": "S256",
    "response_mode": "query",
    "prompt": null
  }
}
```

`claims` 只包含本次 scope 允许且有值的字段。响应包含 `Cache-Control: no-store`。缺少有效登录状态或用户已禁用时返回 `401 Unauthorized` 和 `{"error":"invalid_session"}`。

### 提交授权确认

`POST /oauth2/authorize`

请求使用 `application/x-www-form-urlencoded`，提交与原授权请求相同的全部字段，并增加：

| 字段 | 约束 |
| --- | --- |
| `decision` | `approve` 或 `cancel`。 |
| `csrf_token` | 从授权确认上下文取得，绑定当前用户和完整授权请求，有效期 300 秒。 |

请求体上限为 16 KiB。服务端重新检查 SSO Cookie、用户状态、客户端状态、授权参数和 CSRF token，不信任页面隐藏字段，也不保存历史同意结果。

- `approve` 为本次请求生成一次性授权码，返回 `303 See Other` 到已验证的 `redirect_uri`，并追加 `code` 和原始 `state`。
- `cancel` 返回 `303 See Other`，向已验证的 `redirect_uri` 追加 `error=access_denied` 和原始 `state`。
- 每次授权请求都要经过确认；已有 SSO Cookie 只免去再次输入用户名和密码。

授权结果跳转包含 `Cache-Control: no-store`。

### Token

`POST /oauth2/token`

请求必须使用 `application/x-www-form-urlencoded`：

```text
grant_type=authorization_code&client_id=client_...&redirect_uri=https%3A%2F%2Fclient.example.com%2Fcallback&code=...&code_verifier=...
```

请求体上限为 16 KiB，只能包含以下字段：

| 字段 | 约束 |
| --- | --- |
| `grant_type` | 固定为 `authorization_code`。 |
| `client_id` | 必须与授权码绑定的客户端一致。 |
| `redirect_uri` | 必须与发起授权时使用的实际地址一致。 |
| `code` | 一次性授权码，有效期 120 秒。 |
| `code_verifier` | 43 至 128 个未保留字符，S256 结果必须匹配授权码中的 `code_challenge`。 |

Web 客户端必须同时发送 `Authorization: Basic <base64(client_id:client_secret)>`，使用 `client_secret_basic` 认证并校验 PKCE。Desktop 客户端不得发送客户端认证头，按 `none` 处理，只通过客户端绑定和 PKCE 保护授权码。两类客户端都必须在请求体中发送 `client_id`。

成功响应：

```json
{
  "token_type": "Bearer",
  "expires_in": 300,
  "scope": "openid profile email",
  "access_token": "eyJ...",
  "id_token": "eyJ..."
}
```

响应不包含 `refresh_token`。ID Token 和 access token 都使用 `RS256`，JWT header 携带当前 `kid`。ID Token 包含 `iss`、`aud`、`sub`、`iat`、`exp`、`auth_time` 和授权请求中的 `nonce`，并按 scope 增加身份声明；access token 包含 `iss`、`aud`、`sub`、`client_id`、`scope`、`iat` 和 `exp`。ID Token 的 `aud` 为客户端 ID，access token 的 `aud` 为 `<issuer>/oauth2/userinfo`；两者的 `sub` 都是用户 `employee_id`。

Token 成功和错误响应均包含：

```text
Cache-Control: no-store
Pragma: no-cache
```

### UserInfo

`GET /oauth2/userinfo`

请求必须携带 OIDC access token：

```text
Authorization: Bearer <access_token>
```

服务端校验 JWT 签名、`kid`、issuer、UserInfo audience、有效期和 scope，并重新读取客户端与用户状态。客户端不存在或停用、用户不存在或禁用时，access token 按无效处理。

响应字段由 access token 中的 scope 决定：

```json
{
  "sub": "1001",
  "preferred_username": "zhangsan",
  "name": "张三",
  "email": "zhangsan@example.com",
  "phone_number": "13800000000"
}
```

- `openid` 返回 `sub`。
- `profile` 返回 `preferred_username` 和 `name`。
- `email` 在用户邮箱非空时返回 `email`。
- `phone` 优先使用非空 `mobile`，否则使用非空 `telephone`；两者都为空时省略 `phone_number`。

成功和错误响应均包含 `Cache-Control: no-store`。

### OIDC 错误

未知客户端、停用客户端和未登记的 `redirect_uri` 返回 Center 本地 `400 Bad Request`，响应为 `{"error":"invalid_request"}`，不设置 `Location`。只有客户端和回调地址已验证后，授权错误才通过 `303 See Other` 返回该回调地址：

| `error` | 含义 |
| --- | --- |
| `invalid_request` | 授权参数、确认决定或 CSRF token 非法。 |
| `unsupported_response_type` | `response_type` 不是 `code`。 |
| `invalid_scope` | scope 格式非法、不受支持或超出客户端允许范围。 |
| `interaction_required` | 请求了 `prompt=none`。 |
| `access_denied` | 用户取消授权。 |

Token 端点只返回 JSON，不跳转：

| 状态码 | `error` | 含义 |
| --- | --- | --- |
| `400 Bad Request` | `invalid_request` | Content-Type、表单结构或必填字段非法。 |
| `405 Method Not Allowed` | `invalid_request` | 使用了非 POST 方法。 |
| `401 Unauthorized` | `invalid_client` | Web Basic 认证失败，或客户端类型与认证方式不匹配；响应包含 `WWW-Authenticate: Basic`。 |
| `400 Bad Request` | `unsupported_grant_type` | `grant_type` 不是 `authorization_code`。 |
| `400 Bad Request` | `invalid_grant` | 授权码未知、过期、已使用，绑定字段或 PKCE 不匹配，或兑换时客户端、用户状态不再有效。 |
| `500 Internal Server Error` | `server_error` | 数据库、时间或签名处理失败。 |

UserInfo 对缺少、格式非法、签名错误、过期或状态失效的 token 返回 `401 Unauthorized`、`{"error":"invalid_token"}` 和 `WWW-Authenticate: Bearer error="invalid_token"`。

## 管理入口

管理凭证只用于创建浏览器管理会话，不能直接访问其他管理接口，也不能复用普通用户自助 token。管理会话 Cookie 名为 `adscope_management`，使用 `HttpOnly`、`Secure`、`SameSite=Strict` 和 `Path=/api/admin`，有效期固定为 8 小时。所有管理 Cookie 会话由管理凭证经 HMAC 派生的服务端密钥签名，且每次创建会生成新的随机 CSRF nonce。

### 管理会话

`POST /api/admin/session` 使用一次管理凭证建立会话：

```json
{
  "token": "<management_token>"
}
```

成功时服务端设置 `adscope_management` Cookie，并返回当前会话绑定的 CSRF nonce：

```json
{
  "csrf_token": "random-nonce"
}
```

`GET /api/admin/session` 用现有管理 Cookie 恢复前端会话，并返回同一会话的 `csrf_token`。`DELETE /api/admin/session` 必须同时携带管理 Cookie 和 `X-ADSCOPE-CSRF-Token`，成功后返回 `204 No Content` 并以 `Max-Age=0` 清除该 Cookie。

这三个端点都返回 `Cache-Control: no-store`。除创建端点外，所有 `/api/admin/*` 路由只接受有效的 `adscope_management` Cookie；普通用户 Cookie、普通用户 Bearer token 和旧管理 Bearer token 一律返回 `401 Unauthorized`。所有写方法还必须提供与当前会话 nonce 完全匹配的请求头：

```text
X-ADSCOPE-CSRF-Token: <csrf_token>
```

缺少或不匹配时返回 `403 Forbidden`。管理写入只维护中心当前事实，不直接访问域控。

### OAuth 客户端管理

`GET /api/admin/oauth-clients` 按 `client_id` 升序返回客户端列表：

```json
{
  "clients": [
    {
      "client_id": "client_...",
      "name": "业务门户",
      "client_type": "web",
      "redirect_uris": ["https://client.example.com/callback"],
      "allowed_scopes": ["openid", "profile", "email"],
      "enabled": true
    }
  ]
}
```

列表和普通修改响应不返回 `client_secret` 或 `client_secret_hash`。

`POST /api/admin/oauth-clients` 创建客户端：

```json
{
  "name": "业务门户",
  "client_type": "web",
  "redirect_uris": ["https://client.example.com/callback"],
  "allowed_scopes": ["openid", "profile", "email"],
  "enabled": true
}
```

服务端生成 `client_id`。Web 客户端同时生成 secret；Desktop 客户端的 `client_secret` 为 `null`：

```json
{
  "client": {
    "client_id": "client_...",
    "name": "业务门户",
    "client_type": "web",
    "redirect_uris": ["https://client.example.com/callback"],
    "allowed_scopes": ["openid", "profile", "email"],
    "enabled": true
  },
  "client_secret": "one-time-secret"
}
```

创建响应包含 `Cache-Control: no-store`。Web secret 只在该响应中显示一次。

`PATCH /api/admin/oauth-clients/{client_id}` 更新显示名称、回调地址、允许的 scope 和启用状态：

```json
{
  "name": "业务门户",
  "redirect_uris": ["https://client.example.com/callback"],
  "allowed_scopes": ["openid", "profile"],
  "enabled": false
}
```

更新不接受 `client_id` 或 `client_type`，并保留原 secret 摘要。

`DELETE /api/admin/oauth-clients/{client_id}` 删除客户端，成功返回 `204 No Content`；客户端不存在时返回 `404 Not Found`。

`POST /api/admin/oauth-clients/{client_id}/secret` 为 Web 客户端重新生成 secret：

```json
{
  "client_id": "client_...",
  "client_secret": "new-one-time-secret"
}
```

新 secret 立即替换旧 secret，响应包含 `Cache-Control: no-store`。Desktop 客户端没有 secret，该操作返回 `409 Conflict`；客户端不存在时返回 `404 Not Found`。

客户端字段遵守以下约束：

- `name` 长度为 1 至 100 个字符。
- `client_type` 只能是 `web` 或 `desktop`，创建后不可修改。
- `redirect_uris` 包含 1 至 10 个绝对 URI，每项最长 2048 字节。
- `allowed_scopes` 包含 1 至 4 个互不重复的值，必须包含 `openid`，其他值只能是 `profile`、`email`、`phone`。
- Web 回调使用 HTTPS 并完整匹配登记值；只有显式开启本机开发配置时才接受 HTTP loopback IP。Desktop 回调只接受 HTTP loopback IP，授权请求必须带实际监听端口，该端口可以替换登记地址中的端口。
- 未定义字段、无效 `client_type` 或缺少必填字段由 JSON 请求解析层拒绝；名称、scope 或回调地址等字段值校验失败返回 `400 Bad Request`。目标不存在返回 `404 Not Found`，管理凭证缺失或错误返回 `401 Unauthorized`。

### 域管理

`GET /api/admin/domains` 查询域列表、启用状态和已确认 revision。

`POST /api/admin/domains` 创建域配置：

```json
{
  "id": "domain-a",
  "name": "A 域",
  "enabled": true,
  "mirror_root_dn": "OU=Mirror,DC=a,DC=example,DC=com",
  "quarantine_ou_dn": "OU=Quarantine,DC=a,DC=example,DC=com",
  "upn_suffix": "a.example.com",
  "employee_id_attribute": "employeeID",
  "managed_group_id_attribute": "adminDescription"
}
```

域 ID 已存在时返回 `409 Conflict`，原域配置、Connector key 摘要和 applied revision 保持不变。

`PATCH /api/admin/domains/{domain_id}` 更新域名称、启用状态、镜像根、隔离 OU、UPN 后缀、工号属性和受管组标识属性，并保留域已有的目录与凭据 applied revision。

创建或修改域成功时，中心从系统安全随机源生成新的 32 字节 Connector key，只持久化其摘要。明文 key 只在本次响应中返回，响应包含 `Cache-Control: no-store`：

```json
{
  "domain": {
    "id": "domain-a",
    "name": "A 域",
    "enabled": true,
    "mirror_root_dn": "OU=Mirror,DC=a,DC=example,DC=com",
    "quarantine_ou_dn": "OU=Quarantine,DC=a,DC=example,DC=com",
    "upn_suffix": "a.example.com",
    "employee_id_attribute": "employeeID",
    "managed_group_id_attribute": "adminDescription",
    "applied_directory_revision": 0,
    "applied_credential_revision": 0
  },
  "connector_key": "64-character-lowercase-hex"
}
```

创建和修改请求不得携带 `connector_key` 或 `connector_key_hash`，携带这些未定义字段时返回 `422 Unprocessable Entity`。修改域会立即替换原 Connector key。管理员必须把本次响应中的新 key 配置到对应 Connector 的 `ADSCOPE_CONNECTOR_KEY`。域列表及其他查询响应不返回 Connector key 或其摘要。

### OU 管理

`GET /api/admin/ous/tree` 查询中心 OU 树。

`POST /api/admin/ous` 创建 OU：

```json
{
  "id": "ou-rd",
  "name": "研发部",
  "parent_id": null
}
```

`PATCH /api/admin/ous/{ou_id}` 更新 OU 名称或父级。物理删除 OU 需要先定义用户、子 OU 和组引用处理规则。

### 用户管理

`GET /api/admin/users` 查询用户列表，支持按 `employee_id`、`username`、`organizational_unit_id` 和 `status` 过滤。

`POST /api/admin/users` 创建用户目录事实并初始化凭据：

```json
{
  "employee_id": "1001",
  "username": "zhangsan",
  "display_name": "张三",
  "email": "zhangsan@example.com",
  "mobile": "13800000000",
  "telephone": "021-10000000",
  "organizational_unit_id": "ou-rd",
  "status": "active",
  "initial_password": "InitialPass123!"
}
```

`GET /api/admin/users/{employee_id}` 查询用户详情。

`PATCH /api/admin/users/{employee_id}` 更新用户目录字段。

`POST /api/admin/users/{employee_id}/disable` 禁用用户。

`POST /api/admin/users/{employee_id}/enable` 启用用户。

`POST /api/admin/users/{employee_id}/password-reset` 管理员重置或代设密码：

```json
{
  "new_password": "ResetPass123!"
}
```

管理员重置密码不要求用户当前密码，调用方必须通过受保护管理入口访问。

### 组管理

`GET /api/admin/groups` 查询组列表。

`POST /api/admin/groups` 创建组：

```json
{
  "id": "group-rd",
  "name": "研发部",
  "organizational_unit_id": "ou-rd"
}
```

`GET /api/admin/groups/{group_id}` 查询组详情和成员。

`PATCH /api/admin/groups/{group_id}` 更新组名和目标 OU。

`PUT /api/admin/groups/{group_id}/members` 用完整集合替换组成员：

```json
{
  "member_employee_ids": [
    "1001",
    "1002"
  ]
}
```

组成员集合是事实源，不单独暴露成员增删事件接口。

### 同步状态

`GET /api/admin/sync/domains` 查询各域同步状态。

响应只使用现有域进度和全局 revision 推导：

```json
{
  "domains": [
    {
      "domain_id": "domain-a",
      "enabled": true,
      "applied_directory_revision": 12,
      "applied_credential_revision": 8,
      "directory_lag": 0,
      "credential_lag": 0
    }
  ]
}
```

rebuild 由 Connector 请求中的 `rebuild_directory` 和 `rebuild_credentials` 标志触发。

## Connector 接口

所有 Connector 接口必须携带：

```text
x-adscope-connector-key: <connector-key>
```

服务端按请求 `domain_id` 校验 `domains.connector_key_hash`。未知域、错误 key 或缺少 key 返回 `401 Unauthorized`，域被禁用返回 `403 Forbidden`。

### Connector 同步

`POST /api/connector/sync`

请求：

```json
{
  "domain_id": "domain-a",
  "applied_directory_revision": 10,
  "applied_credential_revision": 7,
  "rebuild_directory": false,
  "rebuild_credentials": false
}
```

响应：

```json
{
  "directory": {
    "server_revision": 12,
    "batch_revision": 12,
    "organizational_units": [],
    "users": [],
    "groups": [],
    "has_more": false
  },
  "credentials": {
    "server_revision": 8,
    "batch_revision": 8,
    "credentials": [],
    "has_more": false
  },
  "directory_config": {
    "domain_id": "domain-a",
    "mirror_root_dn": "OU=Mirror,DC=a,DC=example,DC=com",
    "quarantine_ou_dn": "OU=Quarantine,DC=a,DC=example,DC=com",
    "upn_suffix": "a.example.com",
    "employee_id_attribute": "employeeID",
    "managed_group_id_attribute": "adminDescription"
  }
}
```

凭据响应包含 Connector 可执行的明文密码。Connector 调用主服务 `/api/connector/sync` 时必须走 TLS，并设置 `Cache-Control: no-store`。

### Connector 确认

`POST /api/connector/confirm`

请求：

```json
{
  "domain_id": "domain-a",
  "channel": "directory",
  "target_revision": 12,
  "success": true,
  "error_code": null
}
```

行为：

- `channel` 只能是 `directory` 或 `credential`。
- `success=true` 时推进对应通道的 applied revision。
- `success=false` 时接受失败回报，但不推进 revision。
- 服务端拒绝倒退确认。
- 服务端拒绝超过当前全局 revision 的确认。

响应：

```json
{
  "accepted": true
}
```
