# Adscope 网关

`adscope-gateway` 是自研的 Rust 网关服务。每个 Portal 容器同时承担三件事：

- 托管 Nuxt 静态前端，用户访问的 `/login`、`/authorize`、`/admin` 等页面由网关直接返回。
- 把 `/api/*`、`/oauth2/*`、`/.well-known/*` 转发到 Center；Center 只做 API、OIDC 和数据。
- 以 `PUBLIC_ISSUER` 作为本网络的 OIDC issuer，并在转发时用它覆盖 Host。

Portal 不保存用户、Token 或 OIDC 签名私钥，也不签发自己的 Token；三个 Portal 共用 Center 的一把签名密钥，但 discovery、登录和回调都停留在各自 Portal 域名。

## 构建

先在 `gateway/web` 构建静态前端，再把产物放进 `gateway/static`：

```text
cd gateway/web
bun install --frozen-lockfile
bun run build
mkdir -p ../static
cp -R .output/public/. ../static/
cd ../..
```

然后在仓库根目录执行：

```text
docker build -f gateway/Dockerfile -t ghcr.io/example/adscope-gateway:0.2.0 .
```

镜像内包含 `gateway/static` 的静态构建产物和网关二进制。Center 镜像不再包含前端。

`gateway/static` 是仓库内的发布输入。修改 `gateway/web` 后必须重新生成并提交；发布流水线不会安装前端依赖或在 CI 中构建前端。

## 环境变量

| 变量 | 必填 | 说明 |
| --- | --- | --- |
| `CENTER_URL` | 是 | Center 根地址，例如 `http://center:8080` |
| `PUBLIC_ISSUER` | 是 | 本 Portal 的公开 HTTPS origin，例如 `https://auth-a.example.com` |
| `LISTEN_ADDR` | 否 | 默认 `0.0.0.0:8443` |
| `WEB_ROOT` | 否 | 默认 `/app/web`；本地调试可指向 `gateway/web/.output/public` |
| `LOGIN_ENABLED` | 否 | `true` 或 `false`，默认 `true` |
| `ADMIN_ENABLED` | 否 | `true` 或 `false`，默认 `true` |
| `TLS_CERT_FILE` | 否 | TLS 证书文件路径；与 `TLS_KEY_FILE` 同时设置时监听 HTTPS |
| `TLS_KEY_FILE` | 否 | TLS 私钥文件路径；与 `TLS_CERT_FILE` 同时设置时监听 HTTPS |

`LOGIN_ENABLED=false` 拦截 `/login`、`/authorize`、`/oauth2/authorize`、`/api/auth/login`、`/api/oauth2/authorize/context`；`ADMIN_ENABLED=false` 拦截 `/admin`、`/admin/*`、`/api/admin`、`/api/admin/*`。浏览器路径返回 HTML 403，API 路径返回 JSON 403，其他路径照常处理。

`PUBLIC_ISSUER` 必须是归一化后的 HTTPS origin，不能包含 userinfo、路径、查询参数或片段。Portal 用它覆盖转发到 Center 的 `Host`，不读取客户端 `Host` 或 `X-Forwarded-Host` 作为 issuer。

`TLS_CERT_FILE` 和 `TLS_KEY_FILE` 都不配置时，Portal 监听 HTTP，TLS 由前置反向代理终止；只配置其中一个时启动失败。内网 HTTP 监听只能暴露在本机回环或受信网络。

## 运行

内网入口 A Portal 由前置反向代理终止 TLS 的示例：

```text
docker run -d --name adscope-gateway-sh \
  --network adscope \
  -p 443:8443 \
  -e CENTER_URL=http://center:8080 \
  -e PUBLIC_ISSUER=https://auth-a.example.com \
  -e LOGIN_ENABLED=true \
  -e ADMIN_ENABLED=true \
  ghcr.io/example/adscope-gateway:0.2.0
```

如果 Portal 自己终止 TLS，再同时挂载并设置 `TLS_CERT_FILE` 和 `TLS_KEY_FILE`。三个 Portal 使用同一个镜像，只改 `PUBLIC_ISSUER`、监听方式和入口开关：

| Portal | issuer | 监听/证书 | 登录 | 管理 |
| --- | --- | --- | --- | --- |
| 内网入口 A | `https://auth-a.example.com` | 内部 CA、自签或前置代理 | 开 | 开 |
| 内网入口 B | `https://auth-b.example.com` | 内部 CA、自签或前置代理 | 开 | 开 |
| 外网 | `https://auth-c.example.com` | 公网 CA 或前置代理 | 开 | 关 |

Portal 请求 Center 固定使用 `PUBLIC_ISSUER` 的 host 作为 `Host`；依赖方按所属网络把 issuer 配置为对应 Portal 地址。Syncer 直接访问 Center，不经过 Portal 的 OIDC 路由。
