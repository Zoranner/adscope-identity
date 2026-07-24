# AD 多域组织结构同步

本项目用于把中心服务维护的组织结构、用户信息、组成员关系和密码同步到多个独立 AD 域。中心服务保存权威事实，域内 Connector 主动拉取期望状态，并写入本地域控。

系统不把某一个 AD 域设为主域，也不做域到域复制。目录和密码变更先写入中心服务，再由各域 Connector 同步落地。普通用户通过中心入口查看本人资料、维护联系方式和修改密码；管理员通过管理入口维护组织、用户、组、域配置和同步状态。

## 核心能力

- 以 `employee_id` 作为跨域唯一身份标识。
- 同步 OU、用户、用户状态、组和组成员关系。
- 支持普通用户登录、查看本人资料、维护联系方式和修改本人密码。
- 支持管理员维护域、OU、用户、组、密码重置和同步状态。
- 中心端内置管理 Web，静态文件由 `adss-center` 统一托管。
- Connector 按域主动拉取当前期望状态，并以 revision 确认执行进度。
- 目录和凭据分通道同步，互不阻塞。

## 使用和部署

- [产品概览](docs/guide/overview.md)
- [运行部署](docs/guide/deployment.md)
- [安全要求](docs/guide/security.md)

主服务和 Connector 是两个独立部署单元，各自在自己的运行目录读取 `.env`。示例文件分别位于 [center/.env.example](center/.env.example) 和 [connector/.env.example](connector/.env.example)。管理 Web 位于 [center/web](center/web)，构建后的静态文件由主服务托管。

真实域控连接支持 `ldap://` 或 `ldaps://`。生产环境建议使用 `ldaps://`，或仅在受保护网络内使用 `ldap://`；密码下发和管理面仍必须通过主服务 TLS。

## 参考文档

工程结构、数据模型、API、同步协议和安全实现边界见 [文档索引](docs/README.md) 和 [参考说明](docs/reference/README.md)。
