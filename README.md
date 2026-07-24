# AD 多域组织结构同步

本项目用于把中心服务维护的组织结构、用户信息、组成员关系和密码同步到多个独立 AD 域。中心服务保存权威事实，域内 Agent 主动拉取期望状态，并写入本地域控。

系统不把某一个 AD 域设为主域，也不做域到域复制。目录和密码变更先写入中心服务，再由各域 Agent 同步落地。普通用户通过中心入口查看本人资料、维护联系方式和修改密码；管理员通过管理入口维护组织、用户、组、域配置和同步状态。

## 核心能力

- 以 `employee_id` 作为跨域唯一身份标识。
- 同步 OU、用户、用户状态、组和组成员关系。
- 支持普通用户登录、查看本人资料、维护联系方式和修改本人密码。
- 支持管理员维护域、OU、用户、组、密码重置和同步状态。
- Agent 按域主动拉取当前期望状态，并以 revision 确认执行进度。
- 目录和凭据分通道同步，互不阻塞。

## 快速开始

复制根目录 `.env.example` 为 `.env`，按本地环境调整数据库、密码保护、会话密钥和 Agent 配置。

```text
cp .env.example .env
```

启动主服务：

```text
cargo run -p adss-server
```

Agent dry-run 可用于验证同步协议和本地 state，不写入 AD。默认 `.env.example` 已使用 dry-run 配置。

启动 Agent：

```text
cargo run -p adss-agent
```

真实域控连接支持 `ldap://` 或 `ldaps://`。生产环境建议使用 `ldaps://`，或仅在受保护网络内使用 `ldap://`；密码下发和管理面仍必须通过主服务 TLS。

## 文档

见 [文档总览](docs/README.md)。

## 工程结构

```text
crates/adss-contract     同步契约、目录计划和共享数据结构
crates/adss-persistence  数据库访问和事实源读写
crates/adss-server       中心服务 API、认证、密码加密和同步控制面
crates/adss-agent        域内 Agent、HTTP 客户端、本地 state 和域控写入
```
