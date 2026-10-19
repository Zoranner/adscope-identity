# 运行与部署说明

## 配置文件

Center、Gateway 和 Connector 是三个独立部署单元，部署方式不同：

- Center 使用 [deploy/center/docker-compose.yml](../../deploy/center/docker-compose.yml) 作为 Docker 服务。复制 [deploy/center/.env.example](../../deploy/center/.env.example) 为 `deploy/center/.env`，替换所有占位值后通过 Compose 启动。
- Gateway 使用 `ghcr.io/zoranner/adscope-gateway:<version>` 镜像，每个网关一个容器，环境变量和 TLS 证书分别配置；运行示例见 [gateway/README.md](../../gateway/README.md)。
- Connector 作为 MSI 安装包运行。安装时通过 MSI 参数写入运行配置并注册 Windows 服务。

Center 和 Gateway 加入预先创建的外部 Docker 网络 `adscope`：

```text
docker network create adscope
```

Center 和 Gateway 镜像随版本标签发布到 GitHub Container Registry。Compose 使用固定版本
`ghcr.io/zoranner/adscope-center:0.1.0` 和 `ghcr.io/zoranner/adscope-gateway:0.1.0`，部署时不需要从 GitHub Release 下载并加载镜像 tar 包；Release 中的镜像归档只用于离线或存档。
如果镜像包为私有包，先使用具备 `read:packages` 权限的 GitHub 个人访问令牌登录：

```text
docker login ghcr.io
```

升级镜像时，在 `deploy/center` 目录执行：

```text
docker compose pull center
docker compose up -d center
```

网关升级使用同一版本标签，重建三个网关容器即可；环境变量和证书挂载不变。

Center 的变量说明只维护在 [deploy/center/.env.example](../../deploy/center/.env.example)。网关变量和运行示例见 [gateway/README.md](../../gateway/README.md)。Connector 配置由 MSI 安装参数或配置命令写入，不分发示例文件。

系统环境变量优先级高于进程读取的配置文件。生产环境可以由容器平台、Windows 服务环境或 Secret 管理系统注入普通密钥变量；OIDC RSA 私钥仍通过受限文件挂载。生产密钥不得写入仓库。

示例文件已说明每个变量的用途和是否必填。本文档只说明启动方式和部署边界，避免重复维护变量说明。

## 主服务

主服务是中心 API 和同步控制面。主服务必须配置 `PASSWORD_ENCRYPTION_KEY`、`PASSWORD_HASH_PROVIDER`、`SESSION_KEY`、`MANAGEMENT_TOKEN` 和 `OIDC_ISSUER`。生产部署还应显式设置 `OIDC_LOOPBACK_HTTP=false`。

SQLite 固定使用进程工作目录下的 `data/adscope.db`，首次启动自动创建文件，无需配置数据库地址。本地从项目根目录启动时使用 `data/adscope.db`。Docker 工作目录为 `/app`，通过 `./data:/app/data` 挂载后，数据库位于宿主机部署目录的 `data/adscope.db`。

Center 按单实例部署。不要让多个 Center 副本共享同一个 SQLite 文件。升级前应停止写入并备份 `data/adscope.db`；恢复时必须同时使用原数据库和原 `PASSWORD_ENCRYPTION_KEY`，否则已有密码密文无法解封。

`PASSWORD_ENCRYPTION_KEY` 是主服务内置密码加密使用的高熵密钥。该密钥通过受限 `.env`、系统环境变量、Windows DPAPI 或同等级本机 Secret 保护，不和数据库备份放在同一位置。

普通用户入口为 `/login`，管理入口为 `/admin`，都由网关提供。Center 只提供 API/OIDC 和同步控制面，不再托管前端；网关镜像会构建 `center/web`，并把 `/api/*`、`/oauth2/*`、`/.well-known/*` 转发到 Center，其余路径返回前端文件。

## 可用性与恢复

当前部署目标是“单实例可恢复”，不是多副本高可用：

- Center 进程退出后由 Compose 的 `restart: unless-stopped` 拉起，数据库和密钥不变。
- Center 不可用期间，已经建立的依赖方本地会话可以继续；新登录和 UserInfo 会受影响。
- 升级或恢复前停止写入，备份 `data/adscope.db` 和 `PASSWORD_ENCRYPTION_KEY`，两者分开保存。
- 建议每日备份一次数据库，至少保留最近 7 份；备份前暂停写入，或使用 SQLite 在线备份。
- 恢复时必须使用原数据库和原 `PASSWORD_ENCRYPTION_KEY`；缺少原密钥会导致已有密码密文无法解封。
- 数据库或磁盘损坏时恢复到最近备份，并接受备份点之后的数据丢失。
- 当前不做自动故障切换；需要更高可用时再单独设计数据库和签名密钥的共享方式。

## OIDC 私钥

OIDC 使用至少 2048 位的 RSA 私钥签发 RS256 token。以下命令适用于使用普通 rootful Docker、已安装 OpenSSL 的 Linux 主机。示例在部署目录的 `app/secrets/` 中生成 3072 位 PKCS#8 PEM 私钥，再把文件设为 Center 容器用户 `10001:10001` 只读：

```sh
mkdir -p app/secrets data
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:3072 -out app/secrets/oidc-private-key.pem
sudo chown 10001:10001 app/secrets/oidc-private-key.pem
sudo chmod 0400 app/secrets/oidc-private-key.pem
```

将 `oidc-private-key.pem` 放在 Center 部署目录的 `app/secrets/` 中，并限制为 Center 容器用户可读。镜像固定以 UID/GID `10001:10001` 运行；如部署镜像修改了运行 UID/GID，应同步调整私钥文件属主。rootless Docker 或启用用户命名空间映射时，应使用映射后的宿主机 UID 和 GID。私钥正文不得进入仓库、`.env`、环境变量或镜像。Compose 将该目录只读挂载到 `/run/secrets`，Center 固定读取 `/run/secrets/oidc-private-key.pem`：

```yaml
services:
  center:
    env_file:
      - .env
    volumes:
      - ./data:/data
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
OIDC_ISSUER=https://sso.example.com
OIDC_LOOPBACK_HTTP=false
```

`OIDC_ISSUER` 必须填写对客户端稳定提供的 canonical HTTPS 地址，并与依赖方配置的地址一致。该值只能是 HTTPS origin，不能包含路径、查询参数或片段；不要填写内网入口域名或站点别名。

OIDC issuer 由外网网关服务；三个网关的入口域名和证书可以不同：

| 网关 | 用户入口 | 证书 |
| --- | --- | --- |
| 上海内网 | `https://sso.sh.corp.example` | 内部 CA 或自签，SAN 含入口域名 |
| 武汉内网 | `https://sso.wh.corp.example` | 内部 CA 或自签，SAN 含入口域名 |
| 外网 | `https://sso.example.com` | 公网 CA，SAN 含 `sso.example.com` |

OIDC issuer 由外网网关承载。内网服务器本身可以访问外网和 Center，所以内网 OIDC 客户端直接使用 `https://sso.example.com`；不需要分离 DNS，也不需要把 issuer 域名写进内网证书。内网网关的证书只覆盖自己的入口域名。

内网用户访问内网网关入口时，浏览器必须信任内部 CA 或自签证书；OIDC 客户端访问外网 issuer 时使用系统信任的公网 CA。不能关闭 TLS 校验。

Compose 将宿主机 `8080` 端口发布到 Center 容器的 `8080` 端口。Center 不管理 TLS 证书，OIDC RSA 私钥也不是 TLS 证书，两类密钥应分别生成和保管。

更换 `app/secrets/oidc-private-key.pem` 后，在 `deploy/center` 目录重建 Center 容器，使其重新加载密钥。私钥可能通过原子替换写入，单独执行 `docker compose restart` 不保证容器重新加载新文件。

```text
docker compose up -d --force-recreate center
```

OIDC 私钥不是环境变量，`OIDC_ISSUER` 也不是签名密钥。轮换只替换 PEM 文件并重建 Center，不改数据库、`client_id` 或 client secret。若只是原地覆盖文件，普通 `restart` 也可能重新读取；如果采用原子替换，使用 `--force-recreate` 更确定。当前阶段不需要双 key 或自动轮换：没有合规要求时，一年一次或怀疑泄露时执行即可。

Center 只发布一个活动公钥，不提供新旧密钥并行验证窗口。旧 ID token 和 access token 自签发起固定有效 300 秒：Center 载入新密钥后会立即拒绝旧 access token，仍缓存旧公钥的客户端也会在 token 自签发起 5 分钟到期后拒绝。私钥更换应安排维护窗口，并通知接入系统重新发起 OIDC 授权。

## 网关容器

`adscope-gateway` 是自研 Rust 服务，镜像内同时包含网关二进制和 Nuxt 静态前端。构建：

```text
docker build -f gateway/Dockerfile -t ghcr.io/zoranner/adscope-gateway:0.1.0 .
```

镜像构建时会执行 `center/web` 的 `bun install --frozen-lockfile` 和 `bun run build`，产物复制到 `/app/web`。

网关变量：

| 变量 | 说明 |
| --- | --- |
| `CENTER_URL` | Center 根地址，例如 `http://center:8080` |
| `LISTEN_ADDR` | 默认 `0.0.0.0:8443` |
| `WEB_ROOT` | 镜像内固定为 `/app/web` |
| `LOGIN_ENABLED` | `true` 或 `false` |
| `ADMIN_ENABLED` | `true` 或 `false` |
| `TLS_CERT_FILE` | 证书文件路径 |
| `TLS_KEY_FILE` | 私钥文件路径 |

三个网关使用同一个镜像，只改环境变量和证书挂载。完整运行示例见 [gateway/README.md](../../gateway/README.md)。

## OIDC 网络接入

Center 只配置一个 canonical `OIDC_ISSUER`。三个网关各自使用自己的入口域名和证书，但都把 API/OIDC 转发到同一个 Center 端口；静态前端由各自网关返回：

完整网络拓扑见 [系统架构](../reference/system-architecture.md)。

```text
上海内网：https://sso.sh.corp.example -> 上海内网网关 -> center:8080
武汉内网：https://sso.wh.corp.example -> 武汉内网网关 -> center:8080
外网：    https://sso.example.com       -> 外网网关     -> center:8080
```

上海内网、武汉内网、外网三个登录网关都指向同一个 Center；外网只有一个网关，由上海外网和武汉外网共用。Center 只有一个，不是每个网关一个副本。

网关负责 TLS、静态前端、API/OIDC 转发和两组入口路径判断；登录/管理开关由各网关自己的环境变量控制，不写进 Center 的 `.env`。Center 不参与开关判断。网关服务终止 TLS，证书由容器挂载；Center 本身不管理 TLS 证书。

三个入口域名可以 302 到 canonical issuer；OIDC 客户端不要把入口域名作为 discovery、authorize 或登录地址。当前 `adscope_sso` Cookie 没有 `Domain` 属性，用不同入口域名登录会产生独立登录态。首版不采用每个网关独立 issuer；独立 issuer 会增加多套客户端登记和跨 issuer 人员映射。

### 登录与管理开关

每个网关是独立进程/容器，各自读取两个环境变量。环境变量天然按网关隔离，Center 不识别站点来源；这两个变量属于网关，不属于 Center 的 `.env`。

| 变量 | `false` 时拦截 | `false` 时仍可用 |
| --- | --- | --- |
| `LOGIN_ENABLED` | `/login`、`/authorize`、`/oauth2/authorize`、`/api/auth/login`、`/api/oauth2/authorize/context` | Discovery、JWKS、Token、UserInfo、`/api/me/*`、退出登录 |
| `ADMIN_ENABLED` | `/admin`、`/admin/*`、`/api/admin`、`/api/admin/*` | 用户登录、Token、UserInfo、Connector 同步 |

- 网关命中禁用路由时直接返回 `403`：浏览器路径返回 HTML 错误页，API 路径返回 JSON 错误，不转发给 Center。
- 网关只对这两组路径做判断；其他 API/OIDC 路径转发给 Center，其余路径返回前端静态文件。
- 上海内网、武汉内网网关可以设置 `LOGIN_ENABLED=true`、`ADMIN_ENABLED=true`。
- 外网网关设置 `ADMIN_ENABLED=false`；`LOGIN_ENABLED` 按需要开关。
- `LOGIN_ENABLED=false` 只停止新的登录，不撤销已签发 Token 或依赖方本地会话。
- `ADMIN_ENABLED=false` 只停止管理面访问，不影响用户登录和 Connector 同步。
- Center 不参与开关判断。
- 网关必须能按这两组路径判断；只能整站开关的实现无法独立控制登录和管理。

网关终止 TLS 时会看到密码、Cookie、client secret 和 Token 响应，必须禁止记录或缓存这些内容。Center 故障时，网关返回 502/503，不能在网关本地签发 Token。

当前 Center 不支持 device authorization、MFA、登录限流、Token 撤销、JWKS 双 key 和 Center 多副本。外网入口暴露登录页前，应在外网网关或前置 WAF 增加登录限流，并明确仅密码认证的风险。依赖方接入细节见 [OIDC 统一登录接入](../reference/oidc-integration.md)。

## 发布产物

推送 `v*` 标签时，发布工作流校验 `center/Cargo.toml`、`gateway/Cargo.toml` 和 `connector/Cargo.toml` 版本，构建 Windows MSI、Linux Center 镜像和 Gateway 镜像，推送 GHCR，并在 GitHub Release 上传 Center/Gateway 镜像归档、Connector MSI、`manifest.json` 和 `SHA256SUMS`。Center Compose 使用固定版本 GHCR 镜像，网关使用同一版本镜像，Connector 使用 MSI 安装并注册 Windows 服务。

## 域配置初始化

域配置变更属于受控运维动作，应通过受保护管理入口维护。测试或部署初始化可以通过受控初始化脚本预置域记录，并写入 Connector key 摘要。

直接数据库写入只适用于初始化，不作为普通管理后台入口。

## Connector

Connector 是域内常驻同步进程。安装 MSI 时填写配置后，Connector 会把 Center 的期望状态写入目标 AD。

Connector 至少需要设置域控制器 LDAP 地址：

```text
DOMAIN_CONTROLLER_LDAP_URL=ldap://dc01.rd.kim:389
```

Connector 主机必须加入 `rd.kim` 域，并以 `NetworkService` 服务身份运行。LDAP 地址只接受 `ldap://<FQDN>:389`；Connector 使用主机计算机账号 `RD\<CONNECTOR-HOST>$` 通过 Kerberos GSS-API 访问域控，不支持 IP 地址、LDAP over TLS、StartTLS、Simple Authentication、NTLM 或保存 LDAP 密码。域管理员只向该计算机账号委派镜像根和隔离 OU 的创建、移动、属性写入、组成员写入、禁用和 Reset Password 必要权限。

历史 AD 用户如果尚未写入工号属性，可以在迁移期临时设置 `MATCH_LEGACY_USERS_BY_USERNAME=1`。此时 Connector 仍先按 `employee_id_attribute` 查找用户；找不到时，才按 `sAMAccountName=username` 查找唯一且没有工号属性的 AD 用户，并补写 Center 中的 `employee_id`。迁移完成后应关闭该开关。

本地 state 文件只保存：

```json
{
  "applied_directory_revision": 0,
  "applied_credential_revision": 0
}
```

Connector 在运行目录下自动维护 `adscope-connector-state.json`。文件无法解析时，Connector 可以以 `0/0` 进度和 rebuild flags 重新拉取，并在 confirm 被中心接受后覆盖 state。

## 部署要求

进入真实环境前必须满足：

- 主服务放在 TLS 后面，凭据响应禁止明文 HTTP。
- 生产环境配置本机高熵密码加密密钥。
- `PASSWORD_ENCRYPTION_KEY`、`SESSION_KEY`、`MANAGEMENT_TOKEN` 和 Connector key 通过受限 `.env`、系统环境变量、Windows DPAPI 或等价机制注入；OIDC 私钥通过只读文件挂载。
- 管理面通过内网网关访问，外网网关关闭管理；管理凭证仍需独立保护，不能把 `/api/admin/*` 暴露给普通用户 token。
- 前端静态文件由网关镜像托管，Center 不提供前端，不单独开放 Nuxt 开发服务。
- 外网入口必须在外网网关或前置 WAF 提供登录限流；当前 Center 没有 MFA、登录锁定和 OIDC 审计。
- OIDC 接入点不得持有签名私钥或本地签发 Token；Center 故障时不做兜底认证。
- 上海内网、武汉内网各有一个独立网关；外网共用一个网关，关闭时会影响上海外网和武汉外网的登录入口。
- 三个网关的入口域名和证书可以不同；内网证书只需覆盖入口域名，内网服务器能访问外网 issuer。
- 完成一次 SQLite 备份恢复演练和一次 OIDC 私钥轮换演练。
- 依赖方本地会话设置明确上限；接受 Center 禁用不会立即撤销依赖方会话。
- 仅向 Connector 主机计算机账号委派镜像根和隔离 OU 范围内的必要权限，不使用 `superuser`、本地账户或其他内置本地服务身份。
- 受管用户禁止域内普通 Change Password，并通过 GPO 隐藏 `Ctrl+Alt+Del` 改密入口。
- AD 沙箱域验证 OU、用户、组、成员、禁用、隔离移动和 Reset Password 全链路，并确认 FQDN/SPN、GSS-API 保密层和主机计算机账号权限均符合实际域策略。
