# 运行与部署说明

## 配置文件

Core、Portal 和 Syncer 是三个独立部署单元，部署方式不同：

- Core 使用 [deploy/core/docker-compose.yml](../../deploy/core/docker-compose.yml) 作为 Docker 服务。复制 [deploy/core/.env.example](../../deploy/core/.env.example) 为 `deploy/core/.env`，替换所有占位值后通过 Compose 启动。
- Portal 使用 `ghcr.io/example/adscope-portal:<version>` 镜像，每个 Portal 一个容器，环境变量和 TLS 证书分别配置。每个 Portal 复制 [deploy/portal/.env.example](../../deploy/portal/.env.example) 为 `.env`，按 [deploy/portal/README.md](../../deploy/portal/README.md) 在各自部署主机启动；镜像构建和完整变量说明见 [portal/README.md](../../portal/README.md)。
- Syncer 作为 MSI 安装包运行。安装时通过 MSI 参数写入运行配置并注册 Windows 服务。

同一 Docker 主机上的 Core 和 Portal 如需通过服务名互相访问，可以加入预先创建的外部 Docker 网络 `adscope`：

```text
docker network create adscope
```

跨主机部署时不需要加入该网络，Portal 直接在 `CORE_URL` 中填写 Core 的公开地址 `https://core.example.com`。

Core 和 Portal 镜像随版本标签发布到 GitHub Container Registry。Compose 使用固定版本
`ghcr.io/example/adscope-core:0.2.0` 和 `ghcr.io/example/adscope-portal:0.2.0`，部署时不需要从 GitHub Release 下载并加载镜像 tar 包；Release 中的镜像归档只用于离线或存档。
如果镜像包为私有包，先使用具备 `read:packages` 权限的 GitHub 个人访问令牌登录：

```text
docker login ghcr.io
```

升级镜像时，在 `deploy/core` 目录执行：

```text
docker compose pull core
docker compose up -d core
```

Portal 升级使用同一版本标签，按实际部署数量重建对应容器即可；环境变量和证书挂载不变。

Core 的变量说明只维护在 [deploy/core/.env.example](../../deploy/core/.env.example)。Portal 的部署模板见 [deploy/portal](../../deploy/portal)，镜像构建和完整变量说明见 [portal/README.md](../../portal/README.md)。Syncer 配置由 MSI 安装参数或配置命令写入，不分发示例文件。

系统环境变量优先级高于进程读取的配置文件。生产环境可以由容器平台、Windows 服务环境或 Secret 管理系统注入普通密钥变量；OIDC RSA 私钥仍通过受限文件挂载。生产密钥不得写入仓库。

示例文件已说明每个变量的用途和是否必填。本文档只说明启动方式和部署边界，避免重复维护变量说明。

## 主服务

Core 是 API 和同步控制面。Core 必须配置 `PASSWORD_ENCRYPTION_KEY`、`PASSWORD_HASH_PROVIDER`、`SESSION_KEY`、`MANAGEMENT_TOKEN` 和 `OIDC_ISSUERS`。生产部署还应显式设置 `OIDC_LOOPBACK_HTTP=false`。旧部署过渡期可以只设置 `OIDC_ISSUER`，但它不能与 `OIDC_ISSUERS` 同时存在。

SQLite 固定使用进程工作目录下的 `data/adscope.db`，首次启动自动创建文件，无需配置数据库地址。本地从项目根目录启动时使用 `data/adscope.db`。Docker 工作目录为 `/app`，通过 `./data:/app/data` 挂载后，数据库位于宿主机部署目录的 `data/adscope.db`。

Core 按单实例部署。不要让多个 Core 副本共享同一个 SQLite 文件。升级前应停止写入并备份 `data/adscope.db`；恢复时必须同时使用原数据库和原 `PASSWORD_ENCRYPTION_KEY`，否则已有密码密文无法解封。

`PASSWORD_ENCRYPTION_KEY` 是主服务内置密码加密使用的高熵密钥。该密钥通过受限 `.env`、系统环境变量、Windows DPAPI 或同等级本机 Secret 保护，不和数据库备份放在同一位置。

普通用户入口为 `/login`，管理入口为 `/admin`，都由 Portal 提供。Core 只提供 API/OIDC 和同步控制面，不托管前端；Portal 镜像包含 `portal/static` 的静态构建产物，并把 `/api/*`、`/oauth2/*`、`/.well-known/*` 转发到 Core，其余路径返回前端文件。

## 可用性与恢复

当前部署目标是“单实例可恢复”，不是多副本高可用：

- Core 进程退出后由 Compose 的 `restart: unless-stopped` 拉起，数据库和密钥不变。
- Core 不可用期间，已经建立的依赖方本地会话可以继续；新登录和 UserInfo 会受影响。
- 升级或恢复前停止写入，备份 `data/adscope.db` 和 `PASSWORD_ENCRYPTION_KEY`，两者分开保存。
- 建议每日备份一次数据库，至少保留最近 7 份；备份前暂停写入，或使用 SQLite 在线备份。
- 恢复时必须使用原数据库和原 `PASSWORD_ENCRYPTION_KEY`；缺少原密钥会导致已有密码密文无法解封。
- 数据库或磁盘损坏时恢复到最近备份，并接受备份点之后的数据丢失。
- 当前不做自动故障切换；需要更高可用时再单独设计数据库和签名密钥的共享方式。

## OIDC 私钥

OIDC 使用至少 2048 位的 RSA 私钥签发 RS256 token。以下命令适用于使用普通 rootful Docker、已安装 OpenSSL 的 Linux 主机。示例在部署目录的 `app/secrets/` 中生成 3072 位 PKCS#8 PEM 私钥，再把文件设为 Core 容器用户 `1000:1000` 只读：

```sh
mkdir -p app/secrets data
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:3072 -out app/secrets/oidc-private-key.pem
sudo chown 1000:1000 app/secrets/oidc-private-key.pem
sudo chmod 0400 app/secrets/oidc-private-key.pem
```

将 `oidc-private-key.pem` 放在 Core 部署目录的 `app/secrets/` 中，并限制为 Core 容器用户可读。镜像固定以 UID/GID `1000:1000` 运行；如部署镜像修改了运行 UID/GID，应同步调整私钥文件属主。rootless Docker 或启用用户命名空间映射时，应使用映射后的宿主机 UID 和 GID。私钥正文不得进入仓库、`.env`、环境变量或镜像。Compose 将该目录只读挂载到 `/run/secrets`，Core 固定读取 `/run/secrets/oidc-private-key.pem`：

```yaml
services:
  core:
    env_file:
      - .env
    volumes:
      - ./data:/app/data
      - ./app/secrets:/run/secrets:ro
    ports:
      - "8080:8080"

networks:
  default:
    external: true
    name: adscope
```

`.env` 中的 OIDC 配置如下：

```text
OIDC_ISSUERS=https://auth-a.example.com,https://auth-b.example.com,https://auth-c.example.com
OIDC_LOOPBACK_HTTP=false
```

`OIDC_ISSUERS` 是 Core 接受的 Portal issuer 白名单。每一项必须是 HTTPS origin，不能包含 userinfo、路径、查询参数或片段；Host 统一小写、去掉末尾点和默认 `:443`，非默认端口保留。不要加入 `core.example.com`，也不要在这里填写 Portal 的内网监听地址。

三个 Portal 分别服务不同网络，并且各自是所属网络的 OIDC issuer：

| Portal | issuer | 证书 |
| --- | --- | --- |
| 内网入口 A | `https://auth-a.example.com` | 内部 CA、自签或前置代理，SAN 含 issuer 域名 |
| 内网入口 B | `https://auth-b.example.com` | 内部 CA、自签或前置代理，SAN 含 issuer 域名 |
| 外网 | `https://auth-c.example.com` | 公网 CA 或前置代理，SAN 含 issuer 域名 |

内网 OIDC 客户端使用所属内网 Portal 的 issuer，外网客户端使用 `https://auth-c.example.com`。三个 issuer 共用 Core 的一把 RSA 私钥，JWKS 相同；授权码、ID Token 和 access token 都绑定当前 Portal 的 issuer，不能跨 Portal 兑换或调用 UserInfo。

内网用户访问内网 Portal 时，浏览器必须信任内部 CA 或自签证书；外网客户端使用系统信任的公网 CA。不能关闭 TLS 校验。Portal 可以由前置反向代理终止 TLS，此时 Portal 使用 HTTP 监听；只配置一个 TLS 文件时启动失败。

Compose 将宿主机 `8080` 端口发布到 Core 容器的 `8080` 端口。Core 不管理 TLS 证书，OIDC RSA 私钥也不是 TLS 证书，两类密钥应分别生成和保管。

更换 `app/secrets/oidc-private-key.pem` 后，在 `deploy/core` 目录重建 Core 容器，使其重新加载密钥。私钥可能通过原子替换写入，单独执行 `docker compose restart` 不保证容器重新加载新文件。

```text
docker compose up -d --force-recreate core
```

OIDC 私钥不是环境变量，`OIDC_ISSUERS` 也不是签名密钥。轮换只替换 PEM 文件并重建 Core，不改数据库、`client_id` 或 client secret。若只是原地覆盖文件，普通 `restart` 也可能重新读取；如果采用原子替换，使用 `--force-recreate` 更确定。当前阶段不需要双 key 或自动轮换：没有合规要求时，一年一次或怀疑泄露时执行即可。

Core 只发布一个活动公钥，不提供新旧密钥并行验证窗口。旧 ID token 和 access token 自签发起固定有效 300 秒：Core 载入新密钥后会立即拒绝旧 access token，仍缓存旧公钥的客户端也会在 token 自签发起 5 分钟到期后拒绝。私钥更换应安排维护窗口，并通知接入系统重新发起 OIDC 授权。

## Portal 容器

`adscope-portal` 是自研 Rust 服务，镜像内同时包含 Portal 二进制和 Nuxt 静态前端。构建：

```text
cd portal/web
bun install --frozen-lockfile
bun run build
mkdir -p ../static
cp -R .output/public/. ../static/
cd ../..
docker build -f portal/Dockerfile -t ghcr.io/example/adscope-portal:0.2.0 .
```

静态前端产物先复制到 `portal/static`，镜像构建时直接复制到 `/app/web`。

`portal/static` 是仓库内的发布输入。修改 `portal/web` 后需要重新构建并提交产物；发布流水线不安装前端依赖，也不在 CI 中构建前端。

Portal 变量：

| 变量 | 说明 |
| --- | --- |
| `CORE_URL` | Core 根地址，例如 `http://core:8080` |
| `PUBLIC_ISSUER` | 本 Portal 的公开 HTTPS origin |
| `LISTEN_ADDR` | 固定为 `0.0.0.0:18086` |
| `WEB_ROOT` | 镜像内固定为 `/app/web` |
| `LOGIN_ENABLED` | `true` 或 `false` |
| `ADMIN_ENABLED` | `true` 或 `false` |
| `TLS_CERT_FILE` | 证书文件路径；与 `TLS_KEY_FILE` 同时设置时启用 HTTPS |
| `TLS_KEY_FILE` | 私钥文件路径；与 `TLS_CERT_FILE` 同时设置时启用 HTTPS |

`LISTEN_ADDR` 是容器内部固定监听端口 `18086`；公开 HTTPS 入口由前置代理转发到该端口，不要把内部监听端口直接写进 issuer。

两项 TLS 变量都不设置时 Portal 监听 HTTP，交给前置反向代理终止 TLS；只设置一个时启动失败。各 Portal 使用同一个镜像；按实际数量部署，只有公开域名不同时 `PUBLIC_ISSUER` 才必须不同。当前三种 Portal 的参数见 [deploy/portal/README.md](../../deploy/portal/README.md)。

## OIDC 网络接入

当前拓扑下 Core 在 `OIDC_ISSUERS` 中配置三个 Portal issuer；实现本身支持一个或多个，只部署一个 Portal 时只配置一个。三个 Portal 各自使用自己的公开域名和证书，但都把 API/OIDC 转发到同一个 Core 端口；静态前端由各自 Portal 返回：

完整网络拓扑见 [系统架构](../reference/system-architecture.md)。

```text
内网入口 A：https://auth-a.example.com        -> 内网入口 A Portal -> https://core.example.com
内网入口 B：https://auth-b.example.com     -> 内网入口 B Portal -> https://core.example.com
外网：    https://auth-c.example.com -> 外网 Portal     -> https://core.example.com
```

内网入口 A、内网入口 B、外网三个 Portal 都指向同一个 Core；外网只有一个 Portal，由外网 A和外网 B共用。Core 只有一个，不是每个 Portal 一个副本。四个 Syncer 直接访问 `https://core.example.com`，不经过 Portal 的 OIDC 路由。

Portal 负责 TLS、静态前端、API/OIDC 转发和两组入口路径判断；登录/管理开关由各 Portal 自己的环境变量控制，不写进 Core 的 `.env`。Core 不参与开关判断。Portal 可以自己终止 TLS，也可以由前置反向代理终止；Core 本身不管理 TLS 证书。

每个 Portal 在转发 `/api/*`、`/oauth2/*`、`/.well-known/*` 时，用自己的 `PUBLIC_ISSUER` host 覆盖 `Host`，不信任客户端 Host 或 `X-Forwarded-Host`。当前 `adscope_sso` Cookie 没有 `Domain` 属性，三个 Portal 的登录态互相独立；本次不实现跨 Portal SSO。

### 登录与管理开关

每个 Portal 是独立进程/容器，各自读取两个环境变量。环境变量天然按 Portal 隔离，Core 不识别站点来源；这两个变量属于 Portal，不属于 Core 的 `.env`。

| 变量 | `false` 时拦截 | `false` 时仍可用 |
| --- | --- | --- |
| `LOGIN_ENABLED` | `/login`、`/authorize`、`/oauth2/authorize`、`/api/auth/login`、`/api/oauth2/authorize/context` | Discovery、JWKS、Token、UserInfo、`/api/me/*`、退出登录 |
| `ADMIN_ENABLED` | `/admin`、`/admin/*`、`/api/admin`、`/api/admin/*` | 用户登录、Token、UserInfo、Syncer 同步 |

- Portal 命中禁用路由时直接返回 `403`：浏览器路径返回 HTML 错误页，API 路径返回 JSON 错误，不转发给 Core。
- Portal 只对这两组路径做判断；其他 API/OIDC 路径转发给 Core，其余路径返回前端静态文件。
- 内网入口 A、内网入口 B Portal 设置 `LOGIN_ENABLED=true`、`ADMIN_ENABLED=true`。
- 外网 Portal 设置 `ADMIN_ENABLED=false`。
- `LOGIN_ENABLED=false` 只停止新的登录，不撤销已签发 Token 或依赖方本地会话。
- `ADMIN_ENABLED=false` 只停止管理面访问，不影响用户登录和 Syncer 同步。
- Core 不参与开关判断。
- Portal 必须能按这两组路径判断；只能整站开关的实现无法独立控制登录和管理。

Portal 终止 TLS 时会看到密码、Cookie、client secret 和 Token 响应，必须禁止记录或缓存这些内容。Core 故障时，Portal 返回 502/503，不能在 Portal 本地签发 Token。

当前 Core 不支持 device authorization、MFA、登录限流、Token 撤销、JWKS 双 key 和 Core 多副本。外网入口暴露登录页前，应在外网 Portal 或前置 WAF 增加登录限流，并明确仅密码认证的风险。依赖方接入细节见 [OIDC 统一登录接入](../reference/oidc-integration.md)。

### 多 issuer 迁移顺序

1. Core 升级到支持 `OIDC_ISSUERS` 的版本，启动时自动为旧库的 `oauth_authorization_codes` 补 `issuer` 列。
2. Core 配置三个 Portal issuer，确认 `core.example.com` 的 discovery 和 JWKS 返回 `404`。
3. Portal 逐个升级，配置 `PUBLIC_ISSUER`，按实际部署选择 HTTP 或 HTTPS 监听。
4. 前置反向代理按域名整体转发到 Portal，不按 `/api`、`/oauth2` 拆路径。
5. 依赖方把 issuer 从 `core.example.com` 改为所属 Portal 地址。
6. 验证三个 Portal 的 discovery、登录、Token 和 UserInfo。
7. 验证四个 Syncer 的同步 revision 正常推进，并确认它们仍直接访问 Core。
8. 稳定后移除旧的 `OIDC_ISSUER` 兼容配置。

区域 A、内网入口 B Portal 设置 `ADMIN_ENABLED=true`，外网 Portal 设置 `ADMIN_ENABLED=false`。三个 Portal 独立登录，不共享 Cookie，不实现跨 Portal SSO；三个 issuer 共用 Core 的一把 RSA 私钥，JWKS 内容相同。

## 发布产物

推送 `v*` 标签时，发布工作流校验 `core/Cargo.toml`、`portal/Cargo.toml` 和 `syncer/Cargo.toml` 版本，构建 Windows MSI、Linux Core 镜像和 Portal 镜像，推送 GHCR，并在 GitHub Release 上传 Core/Portal 镜像归档、Syncer MSI、`manifest.json` 和 `SHA256SUMS`。Core Compose 使用固定版本 GHCR 镜像，Portal 使用同一版本镜像，Syncer 使用 MSI 安装并注册 Windows 服务。

## 域配置初始化

域配置变更属于受控运维动作，应通过受保护管理入口维护。测试或部署初始化可以通过受控初始化脚本预置域记录，并写入 Syncer key 摘要。

直接数据库写入只适用于初始化，不作为普通管理后台入口。

## Syncer

Syncer 是域内常驻同步进程。安装 MSI 时填写配置后，Syncer 会把 Core 的期望状态写入目标 AD。

Syncer 至少需要设置域控制器 LDAP 地址：

```text
DOMAIN_CONTROLLER_LDAP_URL=ldap://dc01.example.com:389
```

Syncer 主机必须加入 `example.com` 域，并以 `NetworkService` 服务身份运行。LDAP 地址只接受 `ldap://<FQDN>:389`；Syncer 使用主机计算机账号 `<DOMAIN>\<SYNCER-HOST>$` 通过 Kerberos GSS-API 访问域控，不支持 IP 地址、LDAP over TLS、StartTLS、Simple Authentication、NTLM 或保存 LDAP 密码。域管理员只向该计算机账号委派镜像根和隔离 OU 的创建、移动、属性写入、组成员写入、禁用和 Reset Password 必要权限。

历史 AD 用户如果尚未写入工号属性，可以在迁移期临时设置 `MATCH_LEGACY_USERS_BY_USERNAME=1`。此时 Syncer 仍先按 `employee_id_attribute` 查找用户；找不到时，才按 `sAMAccountName=username` 查找唯一且没有工号属性的 AD 用户，并补写 Core 中的 `employee_id`。迁移完成后应关闭该开关。

本地 state 文件只保存：

```json
{
  "applied_directory_revision": 0,
  "applied_credential_revision": 0
}
```

Syncer 在运行目录下自动维护 `adscope-syncer-state.json`。文件无法解析时，Syncer 可以以 `0/0` 进度和 rebuild flags 重新拉取，并在 confirm 被 Core 接受后覆盖 state。

## 部署要求

进入真实环境前必须满足：

- 主服务放在 TLS 后面，凭据响应禁止明文 HTTP。
- 生产环境配置本机高熵密码加密密钥。
- `PASSWORD_ENCRYPTION_KEY`、`SESSION_KEY`、`MANAGEMENT_TOKEN` 和 Syncer key 通过受限 `.env`、系统环境变量、Windows DPAPI 或等价机制注入；OIDC 私钥通过只读文件挂载。
- 管理面通过内网 Portal 访问，外网 Portal 关闭管理；管理凭证仍需独立保护，不能把 `/api/admin/*` 暴露给普通用户 token。
- 前端静态文件由 Portal 镜像托管，Core 不提供前端，不单独开放 Nuxt 开发服务。
- 外网入口必须在外网 Portal 或前置 WAF 提供登录限流；当前 Core 没有 MFA、登录锁定和 OIDC 审计。
- OIDC 接入点不得持有签名私钥或本地签发 Token；Core 故障时不做兜底认证。
- 内网入口 A、内网入口 B各有一个独立 Portal；外网共用一个 Portal，关闭时会影响外网 A和外网 B的登录入口。
- 三个 Portal 的 issuer 域名和证书可以不同；每个证书覆盖自己的 issuer 域名。
- 完成一次 SQLite 备份恢复演练和一次 OIDC 私钥轮换演练。
- 依赖方本地会话设置明确上限；接受 Core 禁用不会立即撤销依赖方会话。
- 仅向 Syncer 主机计算机账号委派镜像根和隔离 OU 范围内的必要权限，不使用 `superuser`、本地账户或其他内置本地服务身份。
- 受管用户禁止域内普通 Change Password，并通过 GPO 隐藏 `Ctrl+Alt+Del` 改密入口。
- AD 沙箱域验证 OU、用户、组、成员、禁用、隔离移动和 Reset Password 全链路，并确认 FQDN/SPN、GSS-API 保密层和主机计算机账号权限均符合实际域策略。
