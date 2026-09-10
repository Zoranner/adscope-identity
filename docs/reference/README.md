# 参考说明

## 工程结构

```text
center           中心服务 API、认证、密码加密和同步控制面
center/web       Nuxt Web，静态构建后由中心服务托管
connector        域内 Connector、HTTP 客户端、本地 state 和域控写入
crates/protocol  同步契约、目录计划和共享数据结构
crates/store     数据库访问和事实源读写
```

## 阅读顺序

- [系统架构](system-architecture.md)
- [数据模型](data-model.md)
- [API 契约](api-contract.md)
- [Connector 同步协议](connector-sync-protocol.md)
- [安全实现边界](security-boundary.md)

## 本地运行

主服务和 Connector 是两个独立进程，各自在自己的运行目录读取 `.env`。环境变量示例统一维护在部署目录：

```text
cp deploy/center/.env.example deploy/center/.env
msiexec /i adscope-connector-v<version>-windows-x64.msi CENTER_URL=https://center.example.com DIRECTORY_DOMAIN_ID=domain-a CONNECTOR_ACCESS_KEY=<connector-key> DOMAIN_CONTROLLER_LDAP_URL=ldap://dc01.example.com:389
```

启动主服务：

```text
cargo run -p adscope-center
```

构建 Web：

```text
cd center/web
bun install
bun run build
```

主服务默认读取运行目录下的 `web`，在开发仓库中也会读取 `center/web/.output/public`。需要指定静态文件目录时设置 `WEB_ROOT`。普通用户入口为 `/login`，管理入口为 `/admin`。

启动 Connector：

```text
cargo run -p adscope-connector
```

Connector 使用域控制器 LDAP 地址和主机计算机账号执行同步；配置说明见 [部署指南](../guide/deployment.md#connector)。

## 验证命令

Rust 代码修改后执行：

```text
cargo fmt --all
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
```

Web 代码修改后在 `center/web` 下执行：

```text
bun run typecheck
bun run build
```

文档修改至少检查链接、标题、接口路径和术语一致性。文档检查不代表真实 AD、TLS、密钥保护或生产权限已经验收。
