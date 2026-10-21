# OIDC 统一登录接入

## 能力边界

Core 是受限的 OIDC Provider，不是通用身份平台。当前只支持：

- Authorization Code Flow。
- PKCE `S256`，所有客户端都必须提供。
- `openid`、`profile`、`email`、`phone` 四个固定 scope。
- Web confidential client 和 Desktop public client。
- RS256 ID Token 和 access token，有效期均为 5 分钟。

当前支持三个固定 Portal issuer，但不支持动态客户端注册、device authorization、refresh token、token introspection、token revocation、front-channel/back-channel logout、静默登录、跨 Portal SSO、MFA 和 `groups`/`roles` 声明。Discovery 只声明实际支持的能力，依赖方不应根据未声明的端点做兼容猜测。

## Issuer 与端点

Core 只作为后端，不作为 issuer。三个 Portal 分别是所属网络的 OIDC issuer：

```text
内网入口 A：https://auth-a.example.com
内网入口 B：https://auth-b.example.com
外网：    https://auth-c.example.com
```

Core 的 `OIDC_ISSUERS` 是这三个 issuer 的白名单。每个 Portal 用 `PUBLIC_ISSUER` 覆盖转发到 Core 的 Host，Core 按 Host 选择当前 issuer；`core.example.com` 不在白名单，直接访问其 OIDC 路径返回 `404`。issuer 必须是 HTTPS origin，不能包含 userinfo、路径、query 或 fragment。

| 端点 | 路径 |
| --- | --- |
| Discovery | `/.well-known/openid-configuration` |
| 授权 | `/oauth2/authorize` |
| Token | `/oauth2/token` |
| UserInfo | `/oauth2/userinfo` |
| JWKS | `/oauth2/jwks` |

issuer 应使用长期稳定的 DNS 名称。修改 issuer 会改变 Token 的 `iss`，依赖方据此计算的人员键也会随之变化。

三个 Portal 的 issuer 和证书可以完全不同：内网使用内部 CA 或自签证书，外网使用公网 CA 证书。依赖方使用所属网络的 Portal issuer；客户端记录在 Core 中保持全局，不绑定 issuer，由依赖方自己选择 Portal。Portal 入口和证书见 [系统架构](system-architecture.md)。

内网用户访问内网 Portal 时，浏览器必须信任内部 CA 或自签证书；外网客户端使用系统信任的公网 CA。不能关闭 TLS 校验。Portal 可以自己终止 TLS，也可以由前置反向代理终止，此时 Portal 使用 HTTP 监听。

授权码流程由用户浏览器打开所属 Portal 的登录页。三个 Portal 的 Cookie 都是 host-only，独立登录、独立退出；跨 Portal 兑换授权码返回 `invalid_grant`，跨 Portal 使用 access token 返回 `401 invalid_token`。

## 客户端登记

客户端由管理员在 Core 管理端创建，不支持应用自行注册。

| 类型 | 认证 | Redirect URI |
| --- | --- | --- |
| Web | `client_secret_basic` | 绝对 HTTPS URI，scheme、host、port、path、query 精确匹配 |
| Desktop | 无 secret，按 public client 处理 | `http://127.0.0.1:<port>/...` 或 `http://[::1]:<port>/...`，端口可以变化 |

Web client secret 只在创建或重新生成时返回一次，Core 只保存 SHA-256 摘要。重新生成会立即替换旧 secret，不保留并行有效期，轮换需要协调维护窗口。

Desktop 不接受 `localhost` 主机名，只接受 loopback IP。一个客户端可以登记最多 10 个 redirect URI；scope 只能从固定四项中选择并必须包含 `openid`。

## 授权流程

依赖方发起授权时至少提供：

```text
response_type=code
client_id=<registered client id>
redirect_uri=<exact registered uri>
scope=openid profile email phone
state=<non-empty value>
nonce=<non-empty value>
code_challenge=<43 base64url characters>
code_challenge_method=S256
```

`response_mode` 只能省略或使用 `query`。`prompt` 只能省略或使用 `none`；`prompt=none` 不会跳过确认，而是返回 `interaction_required`。每次授权都需要用户在 Portal 确认页确认，Core 不保存历史同意。

用户确认后，Core 向已验证的 redirect URI 返回一次性授权码和原始 `state`。授权码有效期为 120 秒，只能兑换一次。

Token 请求使用 `application/x-www-form-urlencoded`，只包含：

```text
grant_type=authorization_code
client_id=<client id>
redirect_uri=<same redirect uri>
code=<authorization code>
code_verifier=<43..128 RFC 7636 characters>
```

Web 客户端必须在 `Authorization: Basic` 中提交 client secret；Desktop 客户端不得发送客户端认证头。两类客户端都必须在请求体中提交 `client_id` 和与授权请求完全一致的 `redirect_uri`。

Token 响应包含 `access_token`、`token_type=Bearer`、`expires_in=300`、`id_token` 和实际 scope，不包含 `refresh_token`。

## Token 校验与人员键

依赖方校验 ID Token 时应至少检查：

- 使用 JWKS 验证 RS256 签名，header 的 `kid` 与当前 JWKS 匹配。
- `iss` 等于本次使用的 Portal issuer。
- `aud` 等于本客户端 ID。
- `exp`、`iat` 有效，允许最多 30 秒时钟偏差。
- `nonce` 等于本次授权请求生成的值。

当前 ID Token 不包含 `nbf`、`azp`、`groups`、`roles` 或 `email_verified`，依赖方不要强制要求这些字段。遇到未知 `kid` 时应刷新 JWKS；刷新失败时拒绝 Token，不能降级为不验签。

`sub` 直接使用 `employee_id`，是 public、非 pairwise 的稳定人员标识。同一用户通过不同网络或不同客户端登录时应得到相同的 `sub`。工号是业务标识，不是秘密，也不应被当作授权令牌使用。当前自动工号是 6 位顺序号，可猜测；依赖方不得把 `sub` 当作访问凭据。

依赖方可以使用以下声明：

| 声明 | 来源 | 说明 |
| --- | --- | --- |
| `sub` | `employee_id` | 人员主键 |
| `preferred_username` | `username` | 需要 `profile` scope |
| `name` | 显示名 | 需要 `profile` scope |
| `email` | 邮箱 | 需要 `email` scope；没有 `email_verified` |
| `phone_number` | 手机或办公电话 | 需要 `phone` scope |

ID Token 只用于登录时确认人员。Core 的 access token 只用于调用 `/oauth2/userinfo`，不能作为依赖方业务 API 的 Bearer token。依赖方需要长期会话时，应签发自己的会话或设备凭据，并定义有效期、重新验证和撤销策略。

## 会话与撤销边界

- Core 禁用用户后，新的授权、授权码兑换和 UserInfo 会失败。
- 已经签发的 ID Token 无法由 Core 主动撤回，最多继续有效 5 分钟。
- 浏览器 SSO Cookie 默认有效 1 小时；退出登录只清除当前浏览器 Cookie。
- Core 不支持远程撤销单个浏览器会话，也不退出依赖方已经建立的本地会话。
- Core 禁用用户或修改密码不会撤销依赖方自行签发的设备凭据和 Endpoint Token。

依赖方应把 OIDC 登录视为一次人员确认，把自身会话、设备凭据和撤销责任留在依赖方。高风险操作应重新验证或使用更短的本地会话。

管理端把用户改为禁用就是完整操作，不需要额外人工清单。只有要求“已经建立的依赖方会话立即失效”时，才需要依赖方提供撤销接口或使用更短的会话；这是依赖方自己的会话策略。

## 签名密钥

OIDC 使用一把 RSA 私钥签名，JWKS 发布对应公钥。当前私钥是挂载文件，不是环境变量；`OIDC_ISSUERS` 是 issuer 白名单，不是签名密钥，三个 issuer 共用这一把私钥。

轮换就是替换 PEM 文件并重建 Core：

```text
docker compose up -d --force-recreate core
```

不需要修改数据库、`client_id`、client secret 或 Portal issuer 配置。普通 `restart` 可能重新读取原地覆盖的文件；如果文件是原子替换的，使用 `--force-recreate` 更确定。

当前只有一把活动密钥，没有新旧重叠。轮换后新 Token 使用新 `kid`；Core 立即拒绝旧 access token，依赖方刷新 JWKS 后旧 ID Token 也会失效，最长影响约 5 分钟 Token 加 5 分钟 JWKS 缓存。没有合规要求时，一年一次或怀疑泄露时轮换即可；只有要求轮换期间零停机时，才需要双 key/JWKS 重叠。

## Portal 接入

当前只有一个 Core。内网入口 A、内网入口 B各有一个 Portal；外网只有一个 Portal，由外网 A和外网 B共用。四个 Syncer 直接访问 Core，不经过 Portal：

完整网络拓扑见 [系统架构](system-architecture.md)。

```text
内网入口 A客户端
  -> https://auth-a.example.com
  -> 内网入口 A Portal
  -> https://core.example.com

内网入口 B客户端
  -> https://auth-b.example.com
  -> 内网入口 B Portal
  -> https://core.example.com

外网客户端
  -> https://auth-c.example.com
  -> 外网 Portal
  -> https://core.example.com
```

`adscope-portal` 负责 TLS、静态前端、API/OIDC 转发和两组入口路径判断。登录/管理开关由各 Portal 自己的环境变量控制，不依赖 Core 区分来源。Portal 可以自己终止 TLS，也可以由前置反向代理终止后使用 HTTP 监听。

三个 Portal 不是三个 Core，也不各自持有用户目录或签名密钥。它们托管各自的前端，用 `PUBLIC_ISSUER` 覆盖转发 Host，并把 API/OIDC 转发到同一个 Core。

### 登录与管理开关

每个 Portal 是独立进程/容器，各自读取两个环境变量。环境变量天然按 Portal 隔离，Core 不识别站点来源；这两个变量属于 Portal，不属于 Core 的 `.env`。

| 变量 | `false` 时拦截 | `false` 时仍可用 |
| --- | --- | --- |
| `LOGIN_ENABLED` | `/login`、`/authorize`、`/oauth2/authorize`、`/api/auth/login`、`/api/oauth2/authorize/context` | Discovery、JWKS、Token、UserInfo、`/api/me/*`、退出登录 |
| `ADMIN_ENABLED` | `/admin`、`/admin/*`、`/api/admin`、`/api/admin/*` | 用户登录、Token、UserInfo、Syncer 同步 |

- Portal 命中禁用路由时直接返回 `403`：浏览器路径返回 HTML 错误页，API 路径返回 JSON 错误，不转发给 Core。
- Portal 只对这两组路径做判断，其他路径照常转发给 Core。
- 内网入口 A、内网入口 B Portal 设置 `LOGIN_ENABLED=true`、`ADMIN_ENABLED=true`。
- 外网 Portal 设置 `ADMIN_ENABLED=false`。
- `LOGIN_ENABLED=false` 只停止新的登录，不撤销已签发 Token 或依赖方本地会话。
- `ADMIN_ENABLED=false` 只停止管理面访问，不影响用户登录和 Syncer 同步。
- Core 不参与开关判断。
- Portal 必须能按这两组路径判断；只能整站开关的实现无法独立控制登录和管理。

Portal 边界：

- Portal 终止 TLS 时会看到密码、Cookie、client secret 和 Token 响应，必须禁止记录或缓存这些内容。
- Portal 不修改 `iss`、`sub`、`aud`、`nonce` 或签名，不持有 OIDC 签名私钥，不签发本地 Token。
- 不要把 OIDC 签名私钥放在 Portal 上；Portal 的 issuer 身份来自其公开域名和固定 Host，不来自本地签名密钥。

三个 Portal 的 SSO Cookie 都没有 `Domain` 属性，分别独立登录和退出；本次不实现跨 Portal SSO、共享 Cookie 或统一登出。

Core 不终止 TLS。Portal 自己终止 TLS 时持有自己的 issuer 域名证书；由前置反向代理终止时，Portal 只在受信网络或本机回环上使用 HTTP。Core 故障时，Portal 返回 502/503，不能在 Portal 本地签发 Token。

## 当前部署前置条件

以下能力尚未实现，不能按已完成验收：

- device authorization grant。
- MFA、登录限流和账号锁定。
- OIDC 审计事件和按管理员身份的操作审计。
- Token 撤销、introspection 和 refresh token。
- 新旧 JWKS 密钥重叠。
- Core 多副本、数据库主备、PostgreSQL 或共享数据库。
- HSM/KMS 签名。

外网入口暴露登录页前，应在外网 Portal 或前置 WAF 增加登录限流，并明确是否接受仅密码认证的风险。Core 当前是单实例 SQLite，不能通过增加副本获得高可用。

## 依赖方接入要点

- 使用所属 Portal 的 issuer 获取 Discovery；客户端记录保持全局，不把 client 绑定到某个 issuer。
- 使用 Authorization Code + PKCE S256，不实现 device grant 或 refresh 兼容分支。
- 生成并校验 `state` 和 `nonce`，每次授权都接受确认页。
- 回调地址与登记值完全一致；Desktop 使用 loopback IP，不使用 `localhost`。
- Web 在服务端兑换授权码并保护 client secret，不在浏览器中暴露 secret。
- 校验 `iss`、`aud`、`exp`、`iat`、`nonce` 和签名，允许 30 秒时钟偏差。
- 遇到未知 `kid` 时刷新 JWKS，刷新失败则拒绝 Token。
- 只把 `sub` 作为人员主键，不把邮箱、显示名或工号字符串当作授权依据。
- 依赖方自己维护会话、设备凭据和撤销；不依赖 Core 撤销已签发 Token。
- 日志中不记录授权码、client secret、ID Token、access token、SSO Cookie 或完整授权请求。
