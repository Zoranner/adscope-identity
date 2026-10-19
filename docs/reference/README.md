# 参考文档

## 工程结构

```text
center             中心服务 API、认证、OIDC Provider、管理面和同步控制面
center/src/oidc    OIDC 配置、JWT/JWKS、授权参数校验和协议端点
center/src/routes  普通用户、管理会话、管理接口和 Connector 接口
center/web         Nuxt Web，构建进网关镜像
gateway            自研 Rust 网关：TLS、静态前端、API/OIDC 转发、入口开关
connector          域内 Connector、HTTP 客户端、本地 state 和域控写入
crates/protocol    同步契约、目录计划和共享数据结构
crates/store       数据库访问、revision 和 OAuth 持久化
```

## 阅读顺序

- [系统架构](system-architecture.md)
- [OIDC 统一登录接入](oidc-integration.md)
- [数据模型](data-model.md)
- [API 契约](api-contract.md)
- [Connector 同步协议](connector-sync-protocol.md)
- [安全实现边界](security-boundary.md)

## 本地运行

Center、Gateway 和 Connector 是三个独立进程。本地调试从仓库根目录启动 Center 时，将环境变量示例复制为根目录 `.env`，并将 OIDC 私钥放到根目录 `secrets/oidc-private-key.pem`：

```text
cp deploy/center/.env.example .env
mkdir -p secrets
# 将本机调试 OIDC 私钥保存为 secrets/oidc-private-key.pem
cargo run -p adscope-center
```

Docker 部署使用 `deploy/center/.env` 和 `deploy/center/app/secrets/oidc-private-key.pem`：

```text
cp deploy/center/.env.example deploy/center/.env
cd deploy/center
docker compose up -d center
```

如需单独构建 Web：

```text
cd center/web
bun install --frozen-lockfile
bun run build
```

网关镜像托管 `center/web` 的静态构建产物；修改前端后需要重新构建并提交 `gateway/web`，发布流水线不现场构建前端。本地联调时把 `WEB_ROOT` 指向 `center/web/.output/public`，配置 `CENTER_URL`、TLS 证书和私钥后运行 `cargo run -p adscope-gateway`；完整变量见 [网关说明](../../gateway/README.md)。普通用户入口为 `/login`，管理入口为 `/admin`，都由网关提供。

Connector 通过 MSI 安装并注册 Windows 服务。安装示例：

```text
msiexec /i adscope-connector-v<version>-windows-x64.msi CENTER_URL=https://center.example.com DIRECTORY_DOMAIN_ID=domain-a CONNECTOR_ACCESS_KEY=<connector-key> DOMAIN_CONTROLLER_LDAP_URL=ldap://dc01.example.com:389
```

Connector 使用域控制器 LDAP 地址和主机计算机账号执行同步；配置说明见 [部署指南](../guide/deployment.md)。本地调试也可以直接运行 `cargo run -p adscope-connector`。

## 验证命令

Rust 代码修改后执行：

```text
cargo fmt --all
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
```

根 workspace 排除 `crates/protocol` 和 `crates/store`，需要分别执行：

```text
cargo fmt --manifest-path crates/protocol/Cargo.toml
cargo test --manifest-path crates/protocol/Cargo.toml
cargo clippy --manifest-path crates/protocol/Cargo.toml --all-targets --all-features -- -D warnings
cargo fmt --manifest-path crates/store/Cargo.toml
cargo test --manifest-path crates/store/Cargo.toml
cargo clippy --manifest-path crates/store/Cargo.toml --all-targets --all-features -- -D warnings
```

Web 代码修改后在 `center/web` 下执行：

```text
bun run typecheck
bun run build
```

如修改了 `center/web/tests` 下的测试，可直接执行 `bun test`。

文档修改至少检查链接、标题、接口路径和术语一致性。文档检查不代表真实 AD、TLS、密钥保护或生产权限已经验收。
