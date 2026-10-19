# Adscope 网关

`adscope-gateway` 是自研的 Rust 网关服务。每个网关容器同时承担两件事：

- 托管 Nuxt 静态前端，用户访问的 `/login`、`/authorize`、`/admin` 等页面由网关直接返回。
- 把 `/api/*`、`/oauth2/*`、`/.well-known/*` 转发到 Center；Center 只做 API、OIDC 和数据。

网关不保存用户、Token 或 OIDC 签名私钥，也不签发自己的 Token。

## 构建

先在 `center/web` 构建静态前端，再把产物放进 `gateway/web`：

```text
cd center/web
bun install --frozen-lockfile
bun run build
mkdir -p ../../gateway/web
cp -R .output/public/. ../../gateway/web/
cd ../..
```

然后在仓库根目录执行：

```text
docker build -f gateway/Dockerfile -t ghcr.io/zoranner/adscope-gateway:0.2.1 .
```

镜像内包含 `center/web` 的静态构建产物和网关二进制。Center 镜像不再包含前端。

## 环境变量

| 变量 | 必填 | 说明 |
| --- | --- | --- |
| `CENTER_URL` | 是 | Center 根地址，例如 `http://center:8080` |
| `LISTEN_ADDR` | 否 | 默认 `0.0.0.0:8443` |
| `WEB_ROOT` | 否 | 默认 `/app/web`；本地调试可指向 `center/web/.output/public` |
| `LOGIN_ENABLED` | 否 | `true` 或 `false`，默认 `true` |
| `ADMIN_ENABLED` | 否 | `true` 或 `false`，默认 `true` |
| `TLS_CERT_FILE` | 是 | TLS 证书文件路径 |
| `TLS_KEY_FILE` | 是 | TLS 私钥文件路径 |

`LOGIN_ENABLED=false` 拦截 `/login`、`/authorize`、`/oauth2/authorize`、`/api/auth/login`、`/api/oauth2/authorize/context`；`ADMIN_ENABLED=false` 拦截 `/admin`、`/admin/*`、`/api/admin`、`/api/admin/*`。浏览器路径返回 HTML 403，API 路径返回 JSON 403，其他路径照常处理。

## 运行

上海内网网关示例：

```text
docker run -d --name adscope-gateway-sh \
  --network adscope \
  -p 443:8443 \
  -e CENTER_URL=http://center:8080 \
  -e LOGIN_ENABLED=true \
  -e ADMIN_ENABLED=true \
  -e TLS_CERT_FILE=/run/tls/tls.crt \
  -e TLS_KEY_FILE=/run/tls/tls.key \
  -v /srv/adscope/certs/sh-internal:/run/tls:ro \
  ghcr.io/zoranner/adscope-gateway:0.2.1
```

三个网关使用同一个镜像，只改环境变量和证书：

| 网关 | 入口 | 证书 | 登录 | 管理 |
| --- | --- | --- | --- | --- |
| 上海内网 | `sso.sh.corp.example` | 内部 CA 或自签 | 开 | 开 |
| 武汉内网 | `sso.wh.corp.example` | 内部 CA 或自签 | 开 | 开 |
| 外网 | `sso.example.com` | 公网 CA | 按需 | 关 |

网关请求 Center 不附加任何站点标识请求头；三个网关的区别只在各自的环境变量和证书。
