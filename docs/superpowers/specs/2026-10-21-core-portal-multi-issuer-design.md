# Core、Portal 与多 issuer 设计

## 背景与目标

当前实现只有一个 `OIDC_ISSUER`，所有网关共用同一个 issuer。实际部署中，内网入口 A、内网入口 B和外网是三个独立入口，OIDC 客户端必须使用各自网络的 Portal 地址。三个 Portal 的域名不同，登录 Cookie 也不跨域，因此单 issuer 无法让三个入口各自完成 OIDC 登录。

本设计把职责收敛为：

```text
Core    唯一身份数据中心，保存事实、签发 Token
Portal  每个网络一个认证入口，是该网络的 OIDC issuer
Syncer  每个 AD 域一个目录同步器，只和 Core 交互
```

目标：

- 三个 Portal 各自作为 issuer，客户端使用所属网络的 Portal 地址
- Core 不再作为 issuer，只作为后端
- 三个 issuer 共用 Core 的一把 RSA 私钥
- 三个 Portal 独立登录，不共享 Cookie
- OIDC 客户端不绑定 issuer，由依赖方自己选择 Portal

成功标准：

- 三个 Portal 的 discovery 分别返回自己的 issuer
- 每个 Portal 都能独立完成登录、授权、Token 和 UserInfo
- `core.example.com` 的 OIDC 路径返回 404
- 跨 Portal 兑换授权码或使用 access token 被拒绝
- 四个 Syncer 的同步行为不受影响

## 术语与拓扑

| 术语 | 当前代码名 | 数量 | 职责 |
| --- | --- | --- | --- |
| Core | `adscope-core` | 1 | API、OIDC 后端、SQLite、同步控制面 |
| Portal | `adscope-portal` | 3 | TLS 入口、登录/管理页面、OIDC issuer、API/OIDC 转发 |
| Syncer | `adscope-syncer` | 4 | AD 域内服务，拉取期望状态并写入域控 |

代码和镜像的完整重命名不在本次范围，文档先使用 Core、Portal、Syncer 术语。

```text
                              ┌──────────────────┐
                              │      Core        │
                              │  core.example.com    │
                              └────────┬─────────┘
                                       │
              ┌────────────────────────┴────────────────────────┐
              │                                                 │
        Portal（3 个）                                    Syncer（4 个）
       认证入口                                          目录同步器
              │                                                 │
   ┌──────────┼──────────┐              ┌──────────┬───────────┼───────────┐
   │          │          │              │          │           │           │
 auth-a   auth-b  auth-c     内网入口 A域   外网 A域   内网入口 B域   外网 B域
   │          │          │              │          │           │           │
内网入口 A 用户 内网入口 B 用户 外网用户      内网入口 A AD  外网 A AD  内网入口 B AD  外网 B AD
```

Portal 按用户网络划分，所以是 3 个；Syncer 按 AD 域划分，所以是 4 个。外网 Portal 同时服务外网 A和外网 B用户，但两个外网 AD 域仍然各有一个 Syncer。

| 地址 | 角色 | issuer |
| --- | --- | --- |
| `https://core.example.com` | Core 后端 | 否 |
| `https://auth-a.example.com` | 内网入口 A Portal | 是 |
| `https://auth-b.example.com` | 内网入口 B Portal | 是 |
| `https://auth-c.example.com` | 外网 Portal | 是 |

## issuer 解析

Core 从 `OIDC_ISSUERS` 读取允许的 issuer：

```text
OIDC_ISSUERS=https://auth-a.example.com,https://auth-b.example.com,https://auth-c.example.com
```

启动时校验并归一化：

- 必须是 HTTPS origin
- 必须包含 host
- 不能包含 userinfo、路径、查询参数或片段
- host 统一小写
- 去掉域名末尾的点
- 去掉默认 `:443` 端口
- 非默认端口保留
- 同一 host 不允许重复

归一化示例：

```text
https://auth-a.example.com:443  -> https://auth-a.example.com
https://auth-a.example.com      -> https://auth-a.example.com
https://auth-a.example.com:8443 -> https://auth-a.example.com:8443
```

Core 建立 Host 到 issuer 的映射：

```text
auth-a.example.com        -> https://auth-a.example.com
auth-b.example.com     -> https://auth-b.example.com
auth-c.example.com -> https://auth-c.example.com
```

OIDC 请求按 `Host` 选择 issuer。Host 匹配时先归一化，去掉默认 `:443`。

```text
Host: auth-a.example.com
  -> issuer = https://auth-a.example.com

Host: core.example.com
  -> 不在白名单，OIDC 路径返回 404
```

`core.example.com` 不作为 issuer，也不提供默认 issuer 回退。直接访问 Core 的 `/.well-known/openid-configuration`、`/oauth2/jwks`、`/oauth2/authorize`、`/oauth2/token` 或 `/oauth2/userinfo` 时，如果 Host 不在白名单，一律返回 404。

issuer 是每次请求的上下文，不使用全局可变状态。登录、会话和 Connector API 不依赖 issuer；它们是否可访问由网络边界和 Portal 负责。

过渡期兼容模式只用于旧部署升级窗口：只设置 `OIDC_ISSUER` 时，Core 继续按单 issuer 工作，不要求 Host 匹配；迁移完成后必须删除该变量。目标状态只使用 `OIDC_ISSUERS`，并且不把 `core.example.com` 加入列表。

## Portal 转发与 TLS

每个 Portal 配置自己的公开 issuer：

```text
PUBLIC_ISSUER=https://auth-a.example.com
```

`PUBLIC_ISSUER` 必须是归一化后的 HTTPS origin，不能带路径、查询参数或片段。Portal 转发 `/api/*`、`/oauth2/*`、`/.well-known/*` 到 Core 时，必须用它覆盖 `Host`：

```text
Host: auth-a.example.com
```

Portal 不信任客户端传入的 Host，也不读取 `X-Forwarded-Host` 作为 issuer 来源。

Core 未登录时仍然 302 到同源 `/login`。浏览器停留在当前 Portal 域名，由 Portal 返回登录页，因此登录和 Cookie 都在同一个域名下完成。

Portal 可以自己终止 TLS，也可以由前置反向代理终止 TLS。服务本身不要求监听 443：

```text
浏览器
  -> https://auth-a.example.com:443
  -> 反向代理终止 TLS
  -> http://portal-sh:8080
  -> https://core.example.com
  -> Core
```

Portal 的监听规则：

- `LISTEN_ADDR` 控制内网监听地址，默认 `0.0.0.0:8443`
- 同时提供 `TLS_CERT_FILE` 和 `TLS_KEY_FILE` 时监听 HTTPS
- 两者都不提供时监听 HTTP
- 只提供其中一个时启动失败
- `PUBLIC_ISSUER` 与监听端口无关；公开地址是 `https://auth-a.example.com` 时，Host 不包含内网监听端口

反向代理按域名整体转发到 Portal，不按 `/api`、`/oauth2` 拆路径。内网 HTTP 只允许在受信网络或本机回环上暴露，不能直接开放到用户网络。

## 授权码、ID token 与 access token

授权码记录增加 issuer：

```text
authorization_code:
  code_hash
  issuer
  client_id
  employee_id
  redirect_uri
  scopes
  nonce
  code_challenge
  auth_time
  expires_at
```

授权码签发时使用当前请求的 issuer：

```text
Host: auth-a.example.com
  -> issuer = https://auth-a.example.com
  -> 授权码 issuer = https://auth-a.example.com
```

Token 兑换时：

```text
Host: auth-a.example.com
  -> 当前 issuer = https://auth-a.example.com
  -> 授权码 issuer 必须等于当前 issuer
  -> 不匹配返回 invalid_grant
```

授权码一次使用有效。兑换失败时按现有逻辑消费授权码，不保留可重试状态。

Token 规则：

- ID token 的 `iss` 等于当前 issuer
- access token 的 `iss` 等于当前 issuer
- access token 的 `aud` 等于 `{当前 issuer}/oauth2/userinfo`
- 三个 issuer 共用 Core 的一把 RSA 私钥
- 三个 issuer 的 `kid` 和 JWKS 内容相同
- ID token 和 access token 的有效期保持 300 秒
- 密钥轮换通过替换 Core 的 PEM 并重建 Core 完成，三个 issuer 同时切换；旧 token 最长 5 分钟后失效

UserInfo：

- 按 Host 选择 issuer
- 使用当前 issuer 和对应 audience 验签
- 不同 issuer 签发的 access token 不能跨 Portal 使用
- 失败返回 `401 invalid_token`

JWKS 和 discovery：

- OIDC 协议路径要求已知 issuer Host
- `core.example.com` 不是 issuer，直接访问 discovery 或 JWKS 返回 404
- 客户端只能通过 Portal 获取 discovery 和 JWKS
- 三个 Portal 的 discovery 分别返回各自的 issuer 和端点

## 配置

Core：

```text
OIDC_ISSUERS=https://auth-a.example.com,https://auth-b.example.com,https://auth-c.example.com
OIDC_LOOPBACK_HTTP=false
```

过渡期允许在未设置 `OIDC_ISSUERS` 时继续读取旧的 `OIDC_ISSUER`，把它视为单元素 issuer 列表。两者同时存在时启动失败，避免配置歧义。新部署和迁移完成后只使用 `OIDC_ISSUERS`。

Portal：

```text
CENTER_URL=https://core.example.com
PUBLIC_ISSUER=https://auth-a.example.com
LOGIN_ENABLED=true
ADMIN_ENABLED=true
```

外网 Portal 设置 `ADMIN_ENABLED=false`。内网 Portal 可以同时开启登录和管理。

Syncer 的 `CENTER_URL` 继续指向 Core：

```text
CENTER_URL=https://core.example.com
```

Syncer 不使用 OIDC issuer，也不经过 Portal 的 OIDC 路由。

OIDC 客户端记录保持全局，不在客户端记录中绑定 issuer。依赖方使用哪个 Portal，由依赖方自己的 issuer 配置决定。

## 数据模型与迁移

`oauth_authorization_codes` 增加 `issuer` 列：

```sql
issuer TEXT NOT NULL DEFAULT ''
```

新数据库建表时直接包含该列。已有数据库启动时检查表结构，缺少列则执行 `ALTER TABLE`。

旧授权码没有 issuer，兑换时按 issuer 不匹配处理并返回 `invalid_grant`。授权码有效期只有 120 秒，升级后旧记录会自然清空，不需要数据回填。

`AuthorizationCodeRecord`、SeaORM 实体和 Repository 映射同步增加 `issuer`。

## 安全边界

- Core 只信任 `OIDC_ISSUERS` 白名单内的 Host
- Portal 用 `PUBLIC_ISSUER` 覆盖 Host，不信任客户端 Host
- `core.example.com` 不提供 OIDC issuer，不能作为客户端登录地址
- 授权码绑定 issuer，禁止跨 Portal 兑换
- access token 绑定 issuer audience，禁止跨 Portal 使用
- 三个 Portal 的登录 Cookie 都是 host-only，不共享登录态
- 三个公开域名的 TLS 证书分别覆盖自己的域名；证书可以在 Portal 或前置反向代理上终止
- 内网 Portal 可以使用内部 CA 或自签证书，内网浏览器必须信任
- Core 后端只允许 Portal、Syncer 和受信反向代理访问，不能直接暴露给普通用户
- 外网 Portal 关闭管理入口
- Portal 的 HTTP 监听只允许在受信网络或本机回环使用

## 错误处理

| 场景 | 结果 |
| --- | --- |
| OIDC 请求 Host 为空或不在白名单（目标状态） | 404 |
| `OIDC_ISSUERS` 为空或格式非法 | Core 启动失败 |
| 同一 host 配置多次 | Core 启动失败 |
| `OIDC_ISSUERS` 与 `OIDC_ISSUER` 同时设置 | Core 启动失败 |
| Portal 未配置 `PUBLIC_ISSUER` | Portal 启动失败 |
| Portal 只配置一个 TLS 文件 | Portal 启动失败 |
| 授权码 issuer 不匹配 | `invalid_grant` |
| access token issuer 不匹配 | `401 invalid_token` |
| Portal 无法连接 Core | 返回 502/503，不本地签发 Token |

## 测试

Core 配置测试：

- 多个 issuer 正确解析
- 默认 `:443` 被归一化
- 非默认端口保留
- 重复 host、HTTP、路径、查询参数、片段、userinfo 被拒绝
- `OIDC_ISSUERS` 和 `OIDC_ISSUER` 同时设置时启动失败

Core OIDC 测试：

- 每个 Host 的 discovery 返回对应 issuer 和端点
- `core.example.com` 的 discovery 和 JWKS 返回 404
- 每个 issuer 签发的 ID token `iss` 正确
- 每个 issuer 签发的 access token audience 正确
- 跨 issuer 兑换授权码返回 `invalid_grant`
- 跨 issuer 使用 access token 返回 `401 invalid_token`
- UserInfo 在对应 issuer 下正常工作

Portal 测试：

- `PUBLIC_ISSUER` 校验和归一化
- 转发时覆盖 Host
- 客户端传入的 Host 不影响转发值
- HTTP 模式和 HTTPS 模式启动条件正确
- 缺少 `PUBLIC_ISSUER` 时启动失败

部署验证：

- 三个 Portal 的 discovery 分别返回自己的 issuer
- 三个 Portal 的登录页可访问
- 三个 Portal 各自登录、各自退出，不共享 Cookie
- 外网 Portal 的 `/admin` 返回 403
- 内网 Portal 的管理入口正常
- 四个 Syncer 的同步 revision 正常推进

## 部署与迁移顺序

1. Core 升级到支持 `OIDC_ISSUERS` 的版本，执行授权码表迁移
2. Core 配置三个 Portal issuer，确认 `core.example.com` 的 OIDC 路径返回 404
3. Portal 逐个升级，配置 `PUBLIC_ISSUER`，按实际部署选择 HTTP 或 HTTPS 监听
4. 前置反向代理按域名整体转发到 Portal，不做路径拆分
5. 依赖方把 issuer 从 `core.example.com` 改为所属 Portal 地址
6. 验证三个 Portal 的 discovery、登录、Token 和 UserInfo
7. 验证四个 Syncer 的同步状态
8. 稳定后移除旧的 `OIDC_ISSUER` 兼容配置

## 不在本次范围

- 跨 Portal 共享登录态
- 每个 issuer 独立签名密钥
- OIDC 客户端与 issuer 的强制绑定
- 代码、镜像、MSI 服务名重命名不在多 issuer 改造范围；已作为后续独立变更完成 `center/gateway/connector` 到 `core/portal/syncer` 的重命名
- MFA、登录限流、Token 撤销和 OIDC 审计
