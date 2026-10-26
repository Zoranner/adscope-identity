# Adscope Portal

`adscope-portal` 是自研的 Rust 服务。每个 Portal 容器同时承担三件事：

- 托管 Nuxt 静态前端，用户访问的 `/login`、`/authorize`、`/admin` 等页面由 Portal 直接返回。
- 把 `/api/*`、`/oauth2/*`、`/.well-known/*` 转发到 Core；Core 只做 API、OIDC 和数据。
- 以 `PUBLIC_ISSUER` 作为本网络的 OIDC issuer，并在转发时用它覆盖 Host。

Portal 不保存用户、Token 或 OIDC 签名私钥，也不签发自己的 Token；所有 Portal 共用 Core 的一把签名密钥，但 discovery、登录和回调都停留在各自 Portal 域名。

## 构建

先在 `portal/web` 构建静态前端，再把产物放进 `portal/static`：

```text
cd portal/web
bun install --frozen-lockfile
bun run build
mkdir -p ../static
cp -R .output/public/. ../static/
cd ../..
```

然后在仓库根目录执行：

```text
docker build -f portal/Dockerfile -t ghcr.io/zoranner/adscope-portal:0.3.0 .
```

镜像内包含 `portal/static` 的静态构建产物和 Portal 二进制。Core 镜像不包含前端。

`portal/static` 是仓库内的发布输入。修改 `portal/web` 后必须重新生成并提交；发布流水线不会安装前端依赖或在 CI 中构建前端。

## 环境变量

| 变量 | 必填 | 说明 |
| --- | --- | --- |
| `CORE_URL` | 是 | Core 根地址，例如 `http://core:8080` |
| `PUBLIC_ISSUER` | 是 | 本 Portal 的公开 HTTPS origin，例如 `https://auth-a.example.com` |
| `LISTEN_ADDR` | 否 | 固定为 `0.0.0.0:18086`；这是容器内非特权端口，不是公开 issuer 端口 |
| `WEB_ROOT` | 否 | 默认 `/app/web`；本地调试可指向 `portal/web/.output/public` |
| `LOGIN_ENABLED` | 否 | `true` 或 `false`，默认 `true` |
| `ADMIN_ENABLED` | 否 | `true` 或 `false`，默认 `true` |
| `TLS_CERT_FILE` | 否 | TLS 证书文件路径；与 `TLS_KEY_FILE` 同时设置时监听 HTTPS |
| `TLS_KEY_FILE` | 否 | TLS 私钥文件路径；与 `TLS_CERT_FILE` 同时设置时监听 HTTPS |

`LOGIN_ENABLED=false` 拦截 `/login`、`/authorize`、`/oauth2/authorize`、`/api/auth/login`、`/api/oauth2/authorize/context`；`ADMIN_ENABLED=false` 拦截 `/admin`、`/admin/*`、`/api/admin`、`/api/admin/*`。浏览器路径返回 HTML 403，API 路径返回 JSON 403，其他路径照常处理。

`PUBLIC_ISSUER` 必须是归一化后的 HTTPS origin，不能包含 userinfo、路径、查询参数或片段。Portal 用它覆盖转发到 Core 的 `Host`，不读取客户端 `Host` 或 `X-Forwarded-Host` 作为 issuer。

`TLS_CERT_FILE` 和 `TLS_KEY_FILE` 都不配置时，Portal 监听 HTTP，TLS 由前置反向代理终止；只配置其中一个时启动失败。内网 HTTP 监听只能暴露在本机回环或受信网络。

Portal 容器内固定监听 `18086`，前置代理负责公开 HTTPS 入口，并转发到 Portal 的 `18086`；不要把 `18086` 写进 issuer。`18086` 高于 1024，镜像以 UID `1000` 运行时可以直接绑定，不依赖 root 或额外 capability。部署模板会发布宿主机 `18086`，必须通过防火墙或安全组限制来源，不能直接暴露给普通用户网络。

## 运行

仓库提供单 Portal 的 Compose 部署模板，见 [deploy/portal/README.md](../deploy/portal/README.md)。下面的 `docker run` 示例用于本地或临时调试。

由前置反向代理终止 TLS 的示例：

```text
docker run -d --name adscope-portal-a \
  --network adscope \
  -p 127.0.0.1:18086:18086 \
  -e CORE_URL=http://core:8080 \
  -e PUBLIC_ISSUER=https://auth-a.example.com \
  -e LOGIN_ENABLED=true \
  -e ADMIN_ENABLED=true \
  ghcr.io/zoranner/adscope-portal:0.3.0
```

如果 Portal 自己终止 TLS，再同时挂载并设置 `TLS_CERT_FILE` 和 `TLS_KEY_FILE`。所有 Portal 使用同一个镜像，按公开入口修改 `PUBLIC_ISSUER`、监听方式和入口开关。下面只是示例，实际可以部署一个或多个 Portal：

| 示例 | issuer | 监听/证书 | 登录 | 管理 |
| --- | --- | --- | --- | --- |
| 内网入口 A | `https://auth-a.example.com` | 内部 CA、自签或前置代理 | 开 | 开 |
| 内网入口 B | `https://auth-b.example.com` | 内部 CA、自签或前置代理 | 开 | 开 |
| 外网入口 | `https://auth-c.example.com` | 公网 CA 或前置代理 | 开 | 关 |

Portal 请求 Core 固定使用 `PUBLIC_ISSUER` 的 host 作为 `Host`；依赖方把 issuer 配置为自己使用的 Portal 地址。同一公开 origin 下的多个副本复用同一个 `PUBLIC_ISSUER`。Syncer 直接访问 Core，不经过 Portal 的 OIDC 路由。
