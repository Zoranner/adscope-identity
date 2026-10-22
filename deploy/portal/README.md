# Portal 部署模板

`deploy/portal` 是单个 Portal 的部署模板，可以只部署一个，也可以复制多份部署多个。Portal 数量由公开入口数量决定，不要求固定为三个；同一公开 origin 下可以运行多个副本，它们复用同一个 `PUBLIC_ISSUER`。

所有 Portal 共用同一个镜像、同一个内部端口和同一套基础配置。只有公开 origin 不同时，`PUBLIC_ISSUER` 才必须不同；管理入口和 TLS 按该入口的信任边界配置：

| 示例 | `PUBLIC_ISSUER` | `ADMIN_ENABLED` | TLS |
| --- | --- | --- | --- |
| 内网入口 A | `https://auth-a.example.com` | `true` | 内部 CA、自签或前置代理 |
| 内网入口 B | `https://auth-b.example.com` | `true` | 内部 CA、自签或前置代理 |
| 外网入口 | `https://auth-c.example.com` | `false` | 公网 CA 或前置代理 |

## 启动

在目标 Portal 主机的部署目录中准备配置：

```text
cp .env.example .env
mkdir -p certs
```

编辑 `.env`，至少设置：

```text
CORE_URL=https://core.example.com
PUBLIC_ISSUER=https://auth-a.example.com
LISTEN_ADDR=0.0.0.0:18086
ADMIN_ENABLED=true
```

然后启动：

```text
docker compose up -d
```

`CORE_URL` 是 Portal 转发 API/OIDC 时访问的 Core 地址，不是当前 Portal 的公开地址。`PUBLIC_ISSUER` 必须是本 Portal 的 HTTPS origin，并且必须出现在 Core 的 `OIDC_ISSUERS` 中。每个 Portal 的容器内监听端口固定为 `18086`，Compose 只把它绑定到宿主机回环地址。

## TLS 模式

前置反向代理终止 TLS 时，不设置 `TLS_CERT_FILE` 和 `TLS_KEY_FILE`，Portal 使用 HTTP 监听 `18086`。Compose 固定将宿主机 `127.0.0.1:18086` 映射到容器 `18086`，代理应在同一主机或受信网络中访问该地址。代理按域名整体转发 `/`、`/api/*`、`/oauth2/*` 和 `/.well-known/*`，不要单独拆分 OIDC 路径。

Portal 自己终止 TLS 时：

1. 将证书和私钥放入 `deploy/portal/certs/`。
2. 在 `.env` 中同时设置：

```text
TLS_CERT_FILE=/run/certs/tls.crt
TLS_KEY_FILE=/run/certs/tls.key
```

Portal 自己终止 TLS 时，证书仍由容器内的 `18086` 监听加载。若要保持 issuer 不带端口，仍应让前置代理提供公开 HTTPS 入口并转发到 `127.0.0.1:18086`；直接让客户端访问 `18086` 会把端口写进 issuer，需要同步修改所有 OIDC 客户端，不符合当前固定 issuer 的配置。

只设置一个 TLS 文件时 Portal 会拒绝启动。证书文件必须允许容器用户 UID `1000` 读取。模板使用的 `18086` 高于 1024，不依赖 root 或额外 capability。

## 边界

- Portal 没有自己的用户目录、Token 存储或 OIDC 签名私钥。
- 不同公开域名下的 Portal 独立登录，不共享 Cookie，不做跨 Portal SSO。
- Portal 转发到 Core 时用 `PUBLIC_ISSUER` 覆盖 Host，不信任客户端 Host。
- 面向不受信网络的 Portal 必须设置 `ADMIN_ENABLED=false`；只有受信内网入口才开放 `/admin` 和 `/api/admin/*`。
- 同一个公开域名下的多个 Portal 副本可以复用同一个 `PUBLIC_ISSUER`；只有公开域名不同，才需要不同的值。
- Core 的 `OIDC_ISSUERS` 至少包含一个 issuer；部署几个 Portal，就登记几个不同的 issuer。

镜像构建、完整环境变量和本地运行说明见 [portal/README.md](../../portal/README.md)。
