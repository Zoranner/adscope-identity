# Adscope 系统架构

## 定位与边界

Adscope 管理多个独立 Windows Server AD 域中的组织结构、用户、组和密码，并为预登记应用提供受限的 OIDC 统一登录。

系统只有一个 Center、三个登录网关（内网入口 A、内网入口 B、外网）和四个 Connector（四个局域网、四个 AD 域）：

- Center 是账号、目录、凭据、OIDC 客户端和一次性授权码的唯一事实源。
- Connector 是域内执行器，只把 Center 的当前期望状态写入本地 AD 域控。
- 网关是自研 Rust 服务，负责 TLS、托管静态前端、把 API/OIDC 转发到 Center，并执行两组入口路径判断；不是独立身份单元。

Web 是构建进网关镜像的 Nuxt 静态应用，不单独保存业务状态。Center 只提供 API、OIDC 和数据面，不托管前端。

同步方向固定为 Center 到各 AD 域。域内人工修改不回写 Center，也不传播到其他域。没有统一事实源时，不允许让多个域或站点各自写本地主库后再自动合并。

## 网络架构

图 1：Adscope 网络架构

![图 1：Adscope 网络架构](images/adscope-network-architecture.png)

SVG 源文件：[adscope-network-architecture.svg](images/adscope-network-architecture.svg)。

图中的关键关系：

- 内网入口 A、内网入口 B、外网三个网关都指向同一个 Center，不是三个 Center。
- 外网只有一个网关，由外网 A和外网 B共用。
- 网关负责 TLS、静态前端、API/OIDC 转发和登录/管理两组路径判断，不保存用户、Token 或签名私钥。
- 管理面由内网网关的 `ADMIN_ENABLED` 控制；外网网关关闭管理。Connector 使用独立网络、端口或 ACL。
- Connector 是安装在各 AD 域的 Windows 服务，不是容器，也不是网关。
- 每个 Connector 只服务一个 AD 域，主动访问 Center，并主动连接本地域控。
- Center 不主动连接 AD 域，也不访问 Connector 所在网络。
- 管理员从内网网关访问 `/admin` 页面和 `/api/admin/*`；页面由网关提供，API 转发到 Center。如果通过 VPN 接入，VPN 只是进入内网的路径，不是额外管理入口。

### 域名、证书与 issuer

三个网关的入口域名和证书相互独立，不要求同名，也不共享同一张证书：

| 网关 | 用户入口 | 证书 | 承载的 issuer |
| --- | --- | --- | --- |
| 内网入口 A | `auth-a.example.com` | 内部 CA 或自签，SAN 含入口域名 | — |
| 内网入口 B | `auth-b.example.com` | 内部 CA 或自签，SAN 含入口域名 | — |
| 外网 | `sso.example.com` | 公网 CA，SAN 含 `sso.example.com` | `https://sso.example.com` |

三个入口域名只负责把用户带到对应网关。OIDC issuer 是协议标识，不是网关入口域名；系统只使用一个 issuer，固定为外网正式域名，由外网网关承载。内网服务器本身可以访问外网和 Center，因此内网 OIDC 客户端直接使用这个 issuer，不需要分离 DNS，也不需要把 issuer 域名写进内网证书 SAN。这样同一套 `client_id`、`sub` 和 JWKS 可以跨三个入口使用，不需要维护三套客户端登记。

授权码流程由用户浏览器打开 issuer 登录页，所以内网用户浏览器也必须能访问 `https://sso.example.com`；这是部署前置条件。

三个入口域名只作为用户入口，不作为 issuer。严格 OIDC 客户端要求 Discovery 地址与 `issuer` 完全一致，因此 OIDC 客户端统一使用外网正式域名。

外网 DNS 把 `sso.example.com` 解析到外网网关，使用公网 CA 证书。内网网关的证书只覆盖自己的入口域名，三个网关的证书可以完全不同，也不需要共享 TLS 私钥。

内网用户访问内网网关入口时，浏览器必须信任内部 CA 或自签证书，不能关闭 TLS 校验；OIDC 客户端访问外网 issuer 时使用系统信任的公网 CA。入口域名可以 302 到 canonical issuer，但 OIDC 客户端必须直接使用 canonical issuer 获取 Discovery、发起授权和校验 `iss`。

### 登录与管理开关

每个网关是独立部署单元，各自读取两个环境变量。环境变量天然按网关隔离，Center 不识别站点来源。

| 变量 | `false` 时拦截 | `false` 时仍可用 |
| --- | --- | --- |
| `LOGIN_ENABLED` | `/login`、`/authorize`、`/oauth2/authorize`、`/api/auth/login`、`/api/oauth2/authorize/context` | Discovery、JWKS、Token、UserInfo、`/api/me/*`、退出登录 |
| `ADMIN_ENABLED` | `/admin`、`/admin/*`、`/api/admin`、`/api/admin/*` | 用户登录、Token、UserInfo、Connector 同步 |

- 网关命中禁用路由时直接返回 `403`：浏览器路径返回 HTML 错误页，API 路径返回 JSON 错误，不转发给 Center。
- 网关只对这两组路径做判断；其他 API/OIDC 路径转发给 Center，其余路径返回前端静态文件。
- 内网入口 A、内网入口 B网关可以设置 `LOGIN_ENABLED=true`、`ADMIN_ENABLED=true`。
- 外网网关设置 `ADMIN_ENABLED=false`；`LOGIN_ENABLED` 按需要开关。
- `LOGIN_ENABLED=false` 只停止新的登录，不撤销已签发 Token 或依赖方本地会话。
- `ADMIN_ENABLED=false` 只停止管理面访问，不影响用户登录和 Connector 同步。
- Center 不参与开关判断。
- 网关在返回前端或转发 API 前判断这两组路径；只能整站开关的实现无法独立控制登录和管理。

`adscope-gateway` 是仓库内的 Rust crate，用 axum/axum-server 处理 HTTP/TLS，用 reqwest 流式转发，不依赖 nginx/Caddy。

### 网关安全边界

- 网关如果终止 TLS，会看到密码、Cookie、client secret 和 Token 响应，只部署在受控主机上。
- 网关不记录 Authorization、Cookie、授权码、Token 响应正文或完整请求体。
- 网关不修改 `iss`、`sub`、`aud`、`nonce` 或签名，不持有 OIDC 签名私钥，不签发本地 Token。
- 网关到 Center 走内网；Center 故障时，网关通常表现为连接失败或 502/503，不能本地兜底签发 Token。

## 部署单元与职责

| 单元 | 主要职责 | 明确不承担 |
| --- | --- | --- |
| Center | HTTP API、用户登录、管理会话、OIDC Provider、密码保护、同步控制面、SQLite | 域控写入、TLS 证书管理、前端托管、多主复制 |
| Connector（Windows 服务） | 配置、本地 state、HTTP 拉取、LDAP 执行、运行日志 | 业务事实源、用户目录、OIDC 签发 |
| Web | Nuxt 静态前端，构建进网关镜像 | 独立业务状态、独立 API |
| adscope-gateway | TLS 终止、静态前端、API/OIDC 转发、入口开关 | OIDC 签名、身份目录、Token 存储 |

Center 当前按单实例部署。SQLite 文件位于进程工作目录的 `data/adscope.db`，不支持多个 Center 副本共享同一文件并发写入。

## 代码边界

| 仓库内 Crate 或目录 | 职责 |
| --- | --- |
| `adscope-protocol` | 同步对象、请求响应、目录执行计划和共享数据结构 |
| `adscope-store` | 数据库模型、Repository、revision、OAuth 客户端和授权码持久化 |
| `adscope-center` | Center API、认证、OIDC、管理面、密码处理和同步控制面 |
| `adscope-connector` | Windows 服务、配置、本地 state、HTTP 客户端和域控执行 |
| `adscope-gateway` | TLS 终止、静态前端托管、API/OIDC 转发、入口开关 |
| `center/web` | Nuxt 页面，构建进网关镜像 |

`adscope-gateway` 用 axum/axum-server 终止 TLS，用 reqwest 流式转发 `/api/*`、`/oauth2/*`、`/.well-known/*`，其余路径返回 `center/web` 的静态构建产物。它不实现 OIDC 签名、不保存用户/Token/私钥；Center 不托管前端。

中心服务内部按职责分为：

- `routes.rs`：普通用户登录、自助接口、Connector 同步和 API 总路由。
- `routes/admin.rs`：域、OU、用户、组、密码重置和同步状态。
- `routes/management_session.rs`：部署级管理凭证到浏览器会话的转换。
- `routes/oauth_clients.rs`：OIDC 客户端管理。
- `oidc/`：issuer 配置、JWT/JWKS、授权参数校验和协议端点。
- `password/`：密码 verifier 和可逆密码材料保护。
- `session.rs`：用户会话、管理会话和 CSRF 的 HMAC 签发。
- `state.rs`：Repository、密码提供者、会话发行器和 OIDC 服务的组装。

## 身份与信任边界

`employee_id` 是跨域人员主键，由 Center 分配并保持稳定。OIDC 的 `sub` 直接使用 `employee_id`，不做 pairwise。`username`、邮箱、显示名和工号属性都不是跨域人员主键。

系统使用五类互不替代的凭据：

- 普通用户 Bearer token：只用于 `/api/me/*`。
- `adscope_sso` Cookie：只用于浏览器 OIDC 授权流程。
- `adscope_management` Cookie：只用于管理面，写操作还需要 CSRF 请求头。
- Connector key：按域绑定，只用于 `/api/connector/*`。
- OIDC ID Token 和 access token：ID Token 用于依赖方确认人员，access token 只用于 `/oauth2/userinfo`。

管理面没有管理员账号、角色或审批流。部署级 `MANAGEMENT_TOKEN` 只用于建立 8 小时管理会话，浏览器不持久化该 token。

OIDC 客户端由管理员预先登记，不接受动态注册。Web 客户端是 confidential client，使用 `client_secret_basic`；Desktop 客户端是 public client，只接受 loopback redirect，不持有 secret。

## 核心流程

### 目录与凭据同步

Center 在同一个事务中分配 revision，并把 OU、用户、组、组成员或凭据写入当前事实表。Connector 每轮执行：

```text
读取本地 applied revisions
→ POST /api/connector/sync
→ 执行目录批次
→ 目录成功后 POST /api/connector/confirm
→ 执行凭据批次
→ 凭据成功后 POST /api/connector/confirm
→ confirm accepted 后写本地 state
→ 等待下一轮
```

目录和凭据使用独立 revision，互不阻塞。批次只能确认已经完整执行的目标 revision；失败不推进进度。系统保存当前状态，不保存变更日志、旧密码或历史事件。

### 用户自助

用户使用 Center 用户名和密码登录，成功后得到短期 HMAC Bearer token，同时设置 `adscope_sso` Cookie。自助接口只允许查看本人资料、修改 `email`、`mobile`、`telephone` 和修改本人密码。

### 管理会话

管理员提交部署级管理凭证后，Center 签发独立的管理 Cookie 和随机 CSRF nonce。页面刷新通过会话查询恢复状态；所有写操作同时校验 Cookie 和 CSRF 请求头。Center 不保存管理会话记录。

### OIDC 统一登录

Center 只支持 Authorization Code + PKCE S256：

1. 依赖方使用预登记 `client_id`、精确 `redirect_uri`、`state`、`nonce` 和 `code_challenge` 导航到 `/oauth2/authorize`。
2. 浏览器没有有效 SSO Cookie 时先登录；已有 Cookie 时直接进入确认页。
3. 用户确认后，Center 签发 120 秒有效的一次性授权码。
4. 依赖方调用 `/oauth2/token`，提交授权码和 PKCE verifier；Web 客户端同时使用 Basic client secret。
5. Center 签发 5 分钟有效的 ID Token 和 access token。ID Token 的 `sub` 是 `employee_id`，`aud` 是客户端 ID；access token 的 audience 只指向 UserInfo。

Discovery、JWKS 和所有端点都从固定 `OIDC_ISSUER` 生成，不读取 `Host` 或 `X-Forwarded-*`。三个网关的入口域名可以不同，但 issuer 不随入口域名变化。依赖方接入细节见 [OIDC 统一登录接入](oidc-integration.md)。

## 数据与一致性

Center 保存以下当前事实：

- OU 树、用户目录、组和组成员。
- 当前密码 verifier 和受保护密码材料。
- 域配置、Connector key 摘要和各域 applied revision。
- OIDC 客户端配置和等待兑换的一次性授权码。

浏览器 SSO、管理会话、ID Token 和 access token 都是无状态签名数据。数据库不保存会话、Token、历史同意、授权历史或审计事件。

SQLite schema 由 Center 启动时的幂等建表和补列逻辑维护，不是完整迁移框架。升级前应停止写入并备份数据库；恢复时必须同时使用原数据库和原 `PASSWORD_ENCRYPTION_KEY`，否则已有密码密文无法解封。

## 发布与运行

Center 以 Docker 镜像发布到 GHCR，只包含 API/OIDC，Compose 使用固定版本标签和 `./data:/app/data` 持久化 SQLite。网关以独立 Docker 镜像发布，包含 `adscope-gateway` 和 Nuxt 前端；每个网关容器挂载自己的 TLS 证书。Connector 以 Windows MSI 发布，使用 `NetworkService` 运行，通过 `ldap://<FQDN>:389` 和 Kerberos GSS-API 写入 AD。

推送 `v*` 标签时，发布工作流校验 Center、Gateway 和 Connector 版本，构建 Windows MSI、Linux Center 镜像和 Gateway 镜像，推送 GHCR 镜像，并在 GitHub Release 上传 Center/Gateway 镜像归档、Connector MSI、`manifest.json` 和 `SHA256SUMS`。

## 设计取舍

- Center 是唯一事实源，不做多主写入。这样同步和人员身份只有一个判断位置。
- Connector 主动拉取，Center 不访问域内网络。这样网络边界清晰，Connector 可独立部署和重试。
- 目录和凭据分通道，使用 revision 最终一致。这样密码同步失败不会回退目录进度。
- 前端由网关托管，Center 只做 API/OIDC/数据面。这样每个网络入口独立，Center 镜像更小；代价是三个网关镜像都包含同一份静态前端。
- OIDC 运行在 Center 进程内，不引入独立 IdP 服务。这样账号、密码和客户端配置只有一个来源。
- 会话和 Token 无状态，不建会话表。这样实现简单，但代价是不能远程撤销单个浏览器会话或已签发 Token。
- 当前只支持 Authorization Code + PKCE。这样协议面小、验证路径短；device grant、refresh token 和撤销端点需要单独设计。
- 当前使用一把 RSA 签名密钥，不维护历史 JWKS。这样部署简单，但密钥轮换需要维护窗口。
- 当前使用 SQLite 单实例，不引入 PostgreSQL 或共享数据库。这样部署轻，但没有多副本高可用。
- 管理面使用部署级凭证，不建管理员账号体系。这样不增加权限模型，但也没有按管理员的操作审计。
- 当前接受单实例 Center 和人工恢复，优先保证上线和可维护性。高可用是后续独立项目，不通过给 SQLite 增加副本实现。

## 当前不提供的能力

以下能力尚未实现，不应在接入方案或验收材料中声明为已具备：

- Device Authorization Grant、refresh token、token introspection、token revocation。
- front-channel logout、back-channel logout、远程撤销浏览器会话。
- 动态客户端注册、多个 issuer、OIDC Broker 或 SAML/AD FS 联邦。
- MFA、登录限流或账号锁定。
- OIDC 审计事件、按管理员身份的操作审计。
- JWKS 新旧密钥重叠、HSM/KMS 签名。
- PostgreSQL、多副本 Center、数据库主备。
- `groups`、`roles`、`email_verified` 等扩展声明。
- 通过网关本地签发 Token，或在 Center 故障时由网关兜底认证。

## 演进边界

Device grant、MFA、登录限流、审计、密钥轮换、HA 和多 issuer 都是独立的架构项目，不应塞进网关或业务应用。

- 认证协议扩展进入 `center/src/oidc` 和 OAuth 持久化模型。
- 前端和入口开关进入 `gateway/`，不要塞回 Center 或 Connector。
- 会话和撤销能力进入 Center 会话模型，不能由网关模拟。
- 高可用需要先确定数据库和签名密钥的共享方式，不能直接给 SQLite 加副本。
- 多网络扩展继续使用一个 issuer 加多个入口域名。

## 不在范围

- Connector 自动注册和自动发现。
- 复杂角色平台、审批流和密码到期提醒。
- 任务队列、任务领取、历史事件重放和事件溯源。
- 多 Connector 协调。
- AD 内物理删除 OU、组或用户对象。
- 域内人工修改反向同步。
