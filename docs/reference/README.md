# 参考说明

## 工程结构

```text
center           中心服务 API、认证、密码加密和同步控制面
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

主服务和 Connector 是两个独立进程，各自在自己的运行目录读取 `.env`。本地调试时可以从示例文件创建运行配置：

```text
cp center/.env.example <center-runtime-dir>/.env
cp connector/.env.example <connector-runtime-dir>/.env
```

启动主服务：

```text
cargo run -p adss-center
```

启动 Connector：

```text
cargo run -p adss-connector
```

Connector dry-run 可用于验证同步协议和本地 state，不写入 AD。

## 验证命令

Rust 代码修改后执行：

```text
cargo fmt --all
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
```

文档修改至少检查链接、标题、接口路径和术语一致性。文档检查不代表真实 AD、TLS、密钥保护或生产权限已经验收。
