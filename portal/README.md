# Adscope Portal

`adscope-portal` 是 Adscope 的统一登录入口。用户和管理员访问的是 Portal，账号数据、OIDC 签名和同步控制面都在 Core。

## 是什么

每个 Portal 容器同时承担三件事：

- 托管前端页面：用户访问的 `/login`、`/authorize`、`/admin` 等页面由 Portal 直接返回。
- 反向代理：把 `/api/*`、`/oauth2/*`、`/.well-known/*` 转发到 Core；Core 只做 API、OIDC 和数据。
- 绑定 issuer：以 `PUBLIC_ISSUER` 作为本入口的 OIDC issuer，转发时用它覆盖 Host。

Portal 不保存用户、Token 或 OIDC 签名私钥，也不签发自己的 Token。所有入口共用 Core 的一把签名密钥，但 discovery、登录和回调都停留在各自的 Portal 域名。

## 能做什么

- 统一登录：用户在一个 Portal 完成密码登录，依赖方走标准 OIDC 授权码流程接入，不需要各自保存 AD 密码。
- 多入口各自独立：每个公开域名一个 Portal，issuer、证书和开关互不影响，内网入口和外网入口可以用不同策略。
- 登录与管理分开控制：`LOGIN_ENABLED` 控制登录页，`ADMIN_ENABLED` 控制管理面；外网入口可以只保留登录。
- 无状态：Portal 不落库，重建或增加副本不影响账号、密码密文和已签发 Token；数据和签名密钥只在 Core。
- TLS 灵活：可以自己终止 TLS，也可以交给前置反向代理。
- 与域内同步解耦：Syncer 直接访问 Core，不经过 Portal。

## 部署

Portal 以 `ghcr.io/zoranner/adscope-portal:<version>` 镜像发布，每个公开入口一个容器，按入口配置环境变量和证书。仓库提供单 Portal 的 Compose 模板，见 [deploy/portal/README.md](../deploy/portal/README.md)；完整部署步骤见 [运行与部署说明](../docs/guide/deployment.md)。

由前置反向代理终止 TLS 时：

```text
docker run -d --name adscope-portal-a \
  --network adscope \
  -p 127.0.0.1:18086:18086 \
  -e CORE_URL=http://core:8080 \
  -e PUBLIC_ISSUER=https://auth-a.example.com \
  -e LOGIN_ENABLED=true \
  -e ADMIN_ENABLED=true \
  ghcr.io/zoranner/adscope-portal:0.4.0
```

如果 Portal 自己终止 TLS，再同时挂载并设置 `TLS_CERT_FILE` 和 `TLS_KEY_FILE`。所有 Portal 使用同一个镜像，按公开入口调整 `PUBLIC_ISSUER`、监听方式和入口开关：

| 示例 | issuer | 监听/证书 | 登录 | 管理 |
| --- | --- | --- | --- | --- |
| 内网入口 A | `https://auth-a.example.com` | 内部 CA、自签或前置代理 | 开 | 开 |
| 内网入口 B | `https://auth-b.example.com` | 内部 CA、自签或前置代理 | 开 | 开 |
| 外网入口 | `https://auth-c.example.com` | 公网 CA 或前置代理 | 开 | 关 |

Portal 容器内固定监听 `18086`，前置代理负责公开 HTTPS 入口并转发到这个端口；不要把 `18086` 写进 issuer。`18086` 高于 1024，镜像以 UID `1000` 运行时可以直接绑定，不依赖 root 或额外 capability。部署模板会发布宿主机 `18086`，必须通过防火墙或安全组限制来源，不能直接暴露给普通用户网络。

## 配置

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

同一公开 origin 下的多个 Portal 副本复用同一个 `PUBLIC_ISSUER`；依赖方把 issuer 配置为自己使用的 Portal 地址。Syncer 直接访问 Core，不经过 Portal 的 OIDC 路由。
