# LDAPS Agent Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 Agent 在非 dry-run 模式下通过 LDAPS 对受管 AD 对象执行目录和密码同步。

**Architecture:** 先把目录执行计划从“只含 ID 的操作”改成“含当前对象事实的操作”，避免真实 AD 客户端再回查中心。Agent 内新增最小 `LdapDirectoryClient`，负责连接、绑定、搜索、创建、修改、移动和重置密码；dry-run 仍作为显式测试模式保留。

**Tech Stack:** Rust、ldap3、Tokio、现有 `DirectoryClient`/`AgentRuntime`。

---

## 文件结构

- `crates/adss-contract/src/lib.rs`：扩展 `DirectoryOperationTarget`，让计划携带 OU、用户、组和成员集合的当前事实。
- `crates/adss-contract/tests/sync_contract.rs`：验证计划中包含真实 AD 写入所需数据。
- `crates/adss-agent/src/lib.rs`：新增 LDAP 配置、DN/filter/password 编码工具和 `LdapDirectoryClient`。
- `crates/adss-agent/src/main.rs`：非 dry-run 使用 `LdapDirectoryClient`，dry-run 使用 `DryRunDirectoryClient`。
- `crates/adss-agent/tests/http_client_contract.rs`：验证 LDAP 环境变量解析。
- `crates/adss-agent/tests/execution_contract.rs`：验证 DN/filter/password 编码和操作映射。
- `crates/adss-agent/Cargo.toml`、`Cargo.toml`：引入 `ldap3`。
- `docs/operations.md`、`docs/security.md`：更新非 dry-run 运行条件和 LDAPS 配置。

## 任务

- [x] 写失败测试：目录计划必须携带 OU 名称、用户字段、组名称和成员工号。
- [x] 实现最小契约扩展，让计划操作具备真实 AD 写入所需事实。
- [x] 写失败测试：Agent 配置必须解析 LDAP URL、bind DN、bind password 和 insecure TLS 开关。
- [x] 实现 LDAP 配置解析，保留 dry-run 显式路径。
- [x] 写失败测试：DN 转义、filter 转义、AD `unicodePwd` UTF-16LE 编码必须正确。
- [x] 实现 LDAP 辅助编码函数。
- [x] 写失败测试：LDAP 操作映射使用受管 DN、员工号属性、UPN 后缀和组成员 DN。
- [x] 实现 `LdapDirectoryClient` 的连接、绑定、搜索、add、modify、modifydn 和密码重置边界。
- [x] 接入 Agent `main`，非 dry-run 不再直接失败。
- [x] 更新运行和安全文档。
- [x] 执行 `cargo fmt --all`、`cargo test --workspace`、`cargo clippy --all-targets --all-features -- -D warnings`。

## 未覆盖边界

- 本计划本身不替换生产 KMS；后续提交已通过 password envelope provider 边界支持对接 KMS/HSM 适配器。
- 本计划不实现正式数据库迁移工具。
- 本计划不包含真实 AD 沙箱验收；该验收必须在具备域控、证书和委派账号的环境中单独执行。
