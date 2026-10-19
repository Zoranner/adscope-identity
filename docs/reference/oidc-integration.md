# OIDC 统一登录接入

## 能力边界

Center 是受限的 OIDC Provider，不是通用身份平台。当前只支持：

- Authorization Code Flow。
- PKCE `S256`，所有客户端都必须提供。
- `openid`、`profile`、`email`、`phone` 四个固定 scope。
- Web confidential client 和 Desktop public client。
- RS256 ID Token 和 access token，有效期均为 5 分钟。

当前不支持 device authorization、refresh token、token introspection、token revocation、动态客户端注册、front-channel/back-channel logout、静默登录、多个 issuer、MFA 和 `groups`/`roles` 声明。Discovery 只声明实际支持的能力，依赖方不应根据未声明的端点做兼容猜测。

## Issuer 与端点

Center 只使用一个外部配置的 canonical issuer：

```text
OIDC_ISSUER=https://sso.example.com
```

issuer 必须是 HTTPS origin，不能包含 userinfo、路径、query 或 fragment。Discovery、JWKS、授权、Token 和 UserInfo 地址都由该 issuer 生成，不读取请求 `Host`、`Forwarded` 或 `X-Forwarded-*`。

| 端点 | 路径 |
| --- | --- |
| Discovery | `/.well-known/openid-configuration` |
| 授权 | `/oauth2/authorize` |
| Token | `/oauth2/token` |
| UserInfo | `/oauth2/userinfo` |
| JWKS | `/oauth2/jwks` |

canonical issuer 应使用长期稳定的 DNS 名称。修改 issuer 会改变 Token 的 `iss`，依赖方据此计算的人员键也会随之变化。

三个网关的入口域名和证书可以完全不同：内网使用自定义域名和内部 CA/自签证书，外网使用正式域名和公网 CA 证书。issuer 不随入口域名变化，固定为外网正式域名，由外网网关承载。内网服务器本身可以访问外网和 Center，所以内网 OIDC 客户端直接使用这个 issuer，不需要分离 DNS，也不需要把 issuer 域名写进内网证书 SAN。网关入口域名和证书见 [系统架构](system-architecture.md)。

内网用户访问内网网关入口时，浏览器必须信任内部 CA 或自签证书；OIDC 客户端访问外网 issuer 时使用系统信任的公网 CA。不能关闭 TLS 校验。

授权码流程由用户浏览器打开 issuer 登录页，所以内网用户浏览器也必须能访问 canonical issuer；这是部署前置条件。

## 客户端登记

客户端由管理员在 Center 管理端创建，不支持应用自行注册。

| 类型 | 认证 | Redirect URI |
| --- | --- | --- |
| Web | `client_secret_basic` | 绝对 HTTPS URI，scheme、host、port、path、query 精确匹配 |
| Desktop | 无 secret，按 public client 处理 | `http://127.0.0.1:<port>/...` 或 `http://[::1]:<port>/...`，端口可以变化 |

Web client secret 只在创建或重新生成时返回一次，Center 只保存 SHA-256 摘要。重新生成会立即替换旧 secret，不保留并行有效期，轮换需要协调维护窗口。

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

`response_mode` 只能省略或使用 `query`。`prompt` 只能省略或使用 `none`；`prompt=none` 不会跳过确认，而是返回 `interaction_required`。每次授权都需要用户在 Center 确认页确认，Center 不保存历史同意。

用户确认后，Center 向已验证的 redirect URI 返回一次性授权码和原始 `state`。授权码有效期为 120 秒，只能兑换一次。

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
- `iss` 等于配置的 canonical issuer。
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

ID Token 只用于登录时确认人员。Center 的 access token 只用于调用 `/oauth2/userinfo`，不能作为依赖方业务 API 的 Bearer token。依赖方需要长期会话时，应签发自己的会话或设备凭据，并定义有效期、重新验证和撤销策略。

## 会话与撤销边界

- Center 禁用用户后，新的授权、授权码兑换和 UserInfo 会失败。
- 已经签发的 ID Token 无法由 Center 主动撤回，最多继续有效 5 分钟。
- 浏览器 SSO Cookie 默认有效 1 小时；退出登录只清除当前浏览器 Cookie。
- Center 不支持远程撤销单个浏览器会话，也不退出依赖方已经建立的本地会话。
- Center 禁用用户或修改密码不会撤销依赖方自行签发的设备凭据和 Endpoint Token。

依赖方应把 OIDC 登录视为一次人员确认，把自身会话、设备凭据和撤销责任留在依赖方。高风险操作应重新验证或使用更短的本地会话。

管理端把用户改为禁用就是完整操作，不需要额外人工清单。只有要求“已经建立的依赖方会话立即失效”时，才需要依赖方提供撤销接口或使用更短的会话；这是依赖方自己的会话策略。

## 签名密钥

OIDC 使用一把 RSA 私钥签名，JWKS 发布对应公钥。当前私钥是挂载文件，不是环境变量；`OIDC_ISSUER` 是 issuer，不是签名密钥。

轮换就是替换 PEM 文件并重建 Center：

```text
docker compose up -d --force-recreate center
```

不需要修改数据库、`client_id`、client secret 或 `OIDC_ISSUER`。普通 `restart` 可能重新读取原地覆盖的文件；如果文件是原子替换的，使用 `--force-recreate` 更确定。

当前只有一把活动密钥，没有新旧重叠。轮换后新 Token 使用新 `kid`；Center 立即拒绝旧 access token，依赖方刷新 JWKS 后旧 ID Token 也会失效，最长影响约 5 分钟 Token 加 5 分钟 JWKS 缓存。没有合规要求时，一年一次或怀疑泄露时轮换即可；只有要求轮换期间零停机时，才需要双 key/JWKS 重叠。

## 网关接入

当前只有一个 Center。上海内网、武汉内网各有一个登录网关；外网只有一个网关，由上海外网和武汉外网共用：

完整网络拓扑见 [系统架构](system-architecture.md)。

```text
上海内网客户端
  -> https://sso.sh.corp.example
  -> 上海内网网关
  -> center:8080

武汉内网客户端
  -> https://sso.wh.corp.example
  -> 武汉内网网关
  -> center:8080

外网客户端
  -> https://sso.example.com
  -> 外网网关
  -> center:8080
```

`adscope-gateway` 负责 TLS、静态前端、API/OIDC 转发和两组入口路径判断。登录/管理开关由各网关自己的环境变量控制，不依赖 Center 区分来源。网关服务终止 TLS，证书由容器挂载。

三个网关不是三个 Center，也不各自持有用户目录或签名密钥。它们托管各自的前端，并把 API/OIDC 转发到同一个 Center。

### 登录与管理开关

每个网关是独立进程/容器，各自读取两个环境变量。环境变量天然按网关隔离，Center 不识别站点来源；这两个变量属于网关，不属于 Center 的 `.env`。

| 变量 | `false` 时拦截 | `false` 时仍可用 |
| --- | --- | --- |
| `LOGIN_ENABLED` | `/login`、`/authorize`、`/oauth2/authorize`、`/api/auth/login`、`/api/oauth2/authorize/context` | Discovery、JWKS、Token、UserInfo、`/api/me/*`、退出登录 |
| `ADMIN_ENABLED` | `/admin`、`/admin/*`、`/api/admin`、`/api/admin/*` | 用户登录、Token、UserInfo、Connector 同步 |

- 网关命中禁用路由时直接返回 `403`：浏览器路径返回 HTML 错误页，API 路径返回 JSON 错误，不转发给 Center。
- 网关只对这两组路径做判断，其他路径照常转发给 Center。
- 上海内网、武汉内网网关可以设置 `LOGIN_ENABLED=true`、`ADMIN_ENABLED=true`。
- 外网网关设置 `ADMIN_ENABLED=false`；`LOGIN_ENABLED` 按需要开关。
- `LOGIN_ENABLED=false` 只停止新的登录，不撤销已签发 Token 或依赖方本地会话。
- `ADMIN_ENABLED=false` 只停止管理面访问，不影响用户登录和 Connector 同步。
- Center 不参与开关判断。
- 网关必须能按这两组路径判断；只能整站开关的实现无法独立控制登录和管理。

网关边界：

- 网关终止 TLS 时会看到密码、Cookie、client secret 和 Token 响应，必须禁止记录或缓存这些内容。
- 网关不修改 `iss`、`sub`、`aud`、`nonce` 或签名，不持有 OIDC 签名私钥，不签发本地 Token。
- 不要把 OIDC 签名私钥放在网关上，也不要在网关上实现 OIDC Broker 或独立 issuer。

三个入口域名可以 302 到 canonical issuer；OIDC 客户端不要把入口域名作为 discovery、authorize 或 issuer 地址。当前 SSO Cookie 没有 `Domain` 属性，用不同入口域名登录会产生独立登录态。

Center 不终止 TLS。每个网关容器持有自己的入口域名证书；外网网关还必须持有 canonical issuer 域名的证书。Center 故障时，网关返回 502/503，不能在网关本地签发 Token。

## 当前部署前置条件

以下能力尚未实现，不能按已完成验收：

- device authorization grant。
- MFA、登录限流和账号锁定。
- OIDC 审计事件和按管理员身份的操作审计。
- Token 撤销、introspection 和 refresh token。
- 新旧 JWKS 密钥重叠。
- Center 多副本、数据库主备、PostgreSQL 或共享数据库。
- HSM/KMS 签名。

外网入口暴露登录页前，应在外网网关或前置 WAF 增加登录限流，并明确是否接受仅密码认证的风险。Center 当前是单实例 SQLite，不能通过增加副本获得高可用。

## 依赖方接入要点

- 使用 canonical issuer 获取 Discovery；三个入口域名只作为用户入口，不写入依赖方配置。
- 使用 Authorization Code + PKCE S256，不实现 device grant 或 refresh 兼容分支。
- 生成并校验 `state` 和 `nonce`，每次授权都接受确认页。
- 回调地址与登记值完全一致；Desktop 使用 loopback IP，不使用 `localhost`。
- Web 在服务端兑换授权码并保护 client secret，不在浏览器中暴露 secret。
- 校验 `iss`、`aud`、`exp`、`iat`、`nonce` 和签名，允许 30 秒时钟偏差。
- 遇到未知 `kid` 时刷新 JWKS，刷新失败则拒绝 Token。
- 只把 `sub` 作为人员主键，不把邮箱、显示名或工号字符串当作授权依据。
- 依赖方自己维护会话、设备凭据和撤销；不依赖 Center 撤销已签发 Token。
- 日志中不记录授权码、client secret、ID Token、access token、SSO Cookie 或完整授权请求。
