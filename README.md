# Adscope

本项目用于统一管理多个独立 Windows Server AD 域中的组织结构、用户、组和密码。Center 保存统一的账号信息，各域 Connector 定时同步到本地域控。员工继续使用所在域的账号登录，账号和密码变更从中心统一维护。

同步方向为 Center 到各 AD 域。域内人工修改不会回写中心，也不会传播到其他域。

## 主要功能

- 用户自助：使用用户名登录，查看本人资料，维护邮箱、手机和办公电话，修改本人密码。
- 统一登录：为预登记 Web 和桌面应用提供 Authorization Code + PKCE 的 OIDC 登录。
- 账号管理：管理员维护 OU、用户、用户状态、安全组、组成员和密码重置。
- 域管理：管理员添加或修改 AD 域，获取该域 Connector 使用的一次性 key，并查看同步状态。
- 多域同步：Connector 将中心维护的 OU、用户、组、成员关系和密码同步到各自负责的 AD 域。

普通用户入口为 `/login`，管理入口为 `/admin`。实际访问地址由各网关的入口域名决定；OIDC 依赖方使用配置的 canonical issuer。

## 部署方式

Center 以 Docker 服务运行，只提供 API/OIDC，SQLite 数据保存在部署目录的 `data/` 中。前端和管理页面由 `adscope-gateway` 容器托管，每个网关挂载自己的 TLS 证书并为入口提供 HTTPS。

统一登录需要配置 `OIDC_ISSUER` 和 `OIDC_LOOPBACK_HTTP=false`。Docker 部署目录的 `app/secrets/` 存放受限读取的 `oidc-private-key.pem`，Compose 将该目录只读挂载到容器中的 `/run/secrets/oidc-private-key.pem`。

Connector 作为原生 Windows 服务安装在各 AD 域内。每个 Connector 使用对应域生成的 key 访问 Center，并以 `NetworkService` 的计算机账号通过 Kerberos GSS-API 写入 AD；真实模式只接受 `ldap://<FQDN>:389`。

部署前请阅读[运行部署](docs/guide/deployment.md)和[安全要求](docs/guide/security.md)。

## 文档

- [产品概览](docs/guide/overview.md)
- [运行部署](docs/guide/deployment.md)
- [安全要求](docs/guide/security.md)
- [系统架构](docs/reference/system-architecture.md)
- [OIDC 统一登录接入](docs/reference/oidc-integration.md)
- [技术参考](docs/reference/README.md)
