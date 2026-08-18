# 运行与部署说明

## 配置文件

Center 和 Connector 是两个独立部署单元，部署方式不同：

- Center 使用 [deploy/center/docker-compose.yml](../../deploy/center/docker-compose.yml) 作为 Docker 服务。复制 [deploy/center/.env.example](../../deploy/center/.env.example) 为 `deploy/center/.env`，替换所有占位值后通过 Compose 启动。
- Connector 作为原生 Windows 服务运行。复制 [connector/.env.example](../../connector/.env.example) 到 Connector 运行目录并命名为 `.env`，再使用发布包中的服务安装脚本安装。

Center 加入预先创建的外部 Docker 网络 `adscope`：

```text
docker network create adscope
```

Center 镜像随版本标签发布到 GitHub Container Registry。Compose 使用固定版本
`ghcr.io/zoranner/adscope-center:0.1.0`，部署时不需要从 GitHub Release 下载并加载镜像 tar 包。
如果镜像包为私有包，先使用具备 `read:packages` 权限的 GitHub 个人访问令牌登录：

```text
docker login ghcr.io
```

升级镜像时，在 `deploy/center` 目录执行：

```text
docker compose pull center
docker compose up -d center
```

[center/.env.example](../../center/.env.example) 可用于查阅 Center 变量说明，不是 Docker 部署使用的配置文件。

系统环境变量优先级高于进程读取的配置文件。生产环境可以由容器平台、Windows 服务环境或 Secret 管理系统注入普通密钥变量；OIDC RSA 私钥仍通过受限文件挂载。生产密钥不得写入仓库。

示例文件已说明每个变量的用途和是否必填。本文档只说明启动方式和部署边界，避免重复维护变量说明。

## 主服务

主服务是中心 API 和同步控制面。主服务必须配置 `DATABASE_URL`、`PASSWORD_ENCRYPTION_KEY`、`PASSWORD_HASH_PROVIDER`、`SESSION_KEY`、`MANAGEMENT_TOKEN` 和 `OIDC_ISSUER`。生产部署还应显式设置 `OIDC_LOOPBACK_HTTP=false`。

`PASSWORD_ENCRYPTION_KEY` 是主服务内置密码加密使用的高熵密钥。该密钥通过受限 `.env`、系统环境变量、Windows DPAPI 或同等级本机 Secret 保护，不和数据库备份放在同一位置。

Web 使用 Nuxt 静态构建，由主服务统一托管。构建命令在 `center/web` 下执行：

```text
bun install
bun run build
```

构建产物位于 `center/web/.output/public`。开发仓库中主服务会自动读取该目录；部署时也可以把该目录内容复制到主服务运行目录下的 `web` 目录，或通过 `WEB_ROOT` 指定静态文件目录。

主服务会优先匹配 `/api/*`，非 API 请求由 Web 静态文件处理。普通用户入口为 `/login`，管理入口为 `/admin`。未知 `/api/*` 返回 API 404，不会回退到前端页面。

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
OIDC_ISSUER=https://center.example.com
OIDC_LOOPBACK_HTTP=false
```

`OIDC_ISSUER` 必须填写反向代理对外提供的 HTTPS 地址，并与客户端访问的 Center 地址一致。该值只能是 HTTPS origin，不能包含路径、查询参数或片段。

Compose 将宿主机 `8080` 端口发布到 Center 容器的 `8080` 端口。Center 不管理 TLS 证书，OIDC RSA 私钥也不是 TLS 证书，两类密钥应分别生成和保管。

更换 `app/secrets/oidc-private-key.pem` 后，在 `deploy/center` 目录重建 Center 容器，使其重新加载密钥。私钥可能通过原子替换写入，单独执行 `docker compose restart` 不保证容器重新加载新文件。

```text
docker compose up -d --force-recreate center
```

Center 只发布一个活动公钥，不提供新旧密钥并行验证窗口。旧 ID token 和 access token 自签发起固定有效 300 秒：Center 载入新密钥后会立即拒绝旧 access token，仍缓存旧公钥的客户端也会在 token 自签发起 5 分钟到期后拒绝。私钥更换应安排维护窗口，并通知接入系统重新发起 OIDC 授权。

## 域配置初始化

域配置变更属于受控运维动作，应通过受保护管理入口维护。测试或部署初始化可以通过受控初始化脚本预置域记录，并写入 Connector key 摘要。

直接数据库写入只适用于初始化，不作为普通管理后台入口。

## Connector

Connector 是域内常驻同步进程。默认 [adscope-connector .env 示例](../../connector/.env.example) 使用 dry-run，不写入 AD。

启用真实域控写入时，设置：

```text
ADSCOPE_CONNECTOR_DRY_RUN=0
ADSCOPE_LDAP_URL=ldap://dc01.rd.kim:389
```

Connector 主机必须加入 `rd.kim` 域，并以 `NetworkService` 服务身份运行。真实模式只接受 `ldap://<FQDN>:389`；Connector 使用主机计算机账号 `RD\<CONNECTOR-HOST>$` 通过 Kerberos GSS-API 访问域控，不支持 IP 地址、LDAP over TLS、StartTLS、Simple Authentication、NTLM 或保存 LDAP 密码。域管理员只向该计算机账号委派镜像根和隔离 OU 的创建、移动、属性写入、组成员写入、禁用和 Reset Password 必要权限。

历史 AD 用户如果尚未写入工号属性，可以在迁移期临时设置 `ADSCOPE_ADOPT_EXISTING_USERS_BY_USERNAME=1`。此时 Connector 仍先按 `employee_id_attribute` 查找用户；找不到时，才按 `sAMAccountName=username` 查找唯一且没有工号属性的 AD 用户，并补写中心 `employee_id`。迁移完成后应关闭该开关。

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
- 管理入口使用独立保护，不能把 `/api/admin/*` 暴露给普通用户 token。
- 管理 Web 静态文件由主服务托管，不单独开放 Nuxt 开发服务。
- 仅向 Connector 主机计算机账号委派镜像根和隔离 OU 范围内的必要权限，不使用 `superuser`、本地账户或其他内置本地服务身份。
- 受管用户禁止域内普通 Change Password，并通过 GPO 隐藏 `Ctrl+Alt+Del` 改密入口。
- AD 沙箱域验证 OU、用户、组、成员、禁用、隔离移动和 Reset Password 全链路，并确认 FQDN/SPN、GSS-API 保密层和主机计算机账号权限均符合实际域策略。
