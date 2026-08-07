# 管理会话与 Kerberos Connector 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**目标：** 管理台刷新后通过受保护 Cookie 恢复会话，Connector 以 NetworkService 计算机身份通过 389 端口的 Kerberos GSS-API 同步 AD，并收紧用户创建与目录分页的一致性边界。

**架构：** Center 使用无状态、独立用途签名的管理 Cookie；管理根密钥只参与会话建立，所有管理业务路由经统一 Cookie 和 CSRF 校验。Connector 移除 Simple Bind 配置，在每个非空同步批次建立一条 GSS-API 保护连接并复用。目录分页按 revision 边界返回变更对象与其必需的 OU DN 上下文，避免把全量 OU 树塞进每个响应。

**技术栈：** Rust 2024、Axum 0.8、axum-extra Cookie、HMAC-SHA256、SeaORM 2、ldap3 0.12.1 `gssapi`、Windows Service、Nuxt 4、Vue 3、Bun、PowerShell 7。

---

## 文件职责

- `center/src/session.rs`：普通用户与管理会话的签发、验证和过期规则。
- `center/src/routes/management_session.rs`：管理会话建立、恢复和退出端点。
- `center/src/routes.rs`、`center/src/routes/admin.rs`、`center/src/routes/oauth_clients.rs`：统一管理路由保护和业务路由。
- `center/web/app/composables/useAdminApi.ts`、`center/web/app/components/admin/AdminShell.vue`：浏览器会话恢复、CSRF 请求头和退出交互。
- `connector/src/config.rs`、`connector/src/directory/*`、`connector/src/runtime.rs`：GSS-API 配置与批次 LDAP 会话。
- `deploy/connector/install-service.ps1`：NetworkService 注册与运行目录 ACL。
- `crates/protocol/src/lib.rs`、`crates/store/src/repository.rs`、`connector/src/directory/mod.rs`：有界目录批次与 OU DN 上下文。
- `docs/guide/*`、`docs/reference/*`、`.env.example`：部署、认证和验收契约。

本计划不含 Git 提交步骤，因为尚未获得提交授权。

### 任务：管理会话签发与统一保护

**文件：**
- 修改：`center/src/session.rs`
- 修改：`center/src/state.rs`
- 创建：`center/src/routes/management_session.rs`
- 修改：`center/src/routes.rs`
- 修改：`center/src/routes/admin.rs`
- 修改：`center/src/routes/oauth_clients.rs`
- 测试：`center/src/session.rs`
- 测试：`center/tests/api_contract.rs`
- 测试：`center/tests/oidc_contract.rs`

- [ ] **步骤：为管理 Cookie 会话写失败测试**

在 `center/src/session.rs` 的测试模块添加管理会话回环、篡改、到期和不同 token 前缀拒绝测试。新增的发行器接口固定为：

```rust
let issuer = ManagementSessionIssuer::from_management_token("management-secret");
let token = issuer.issue_at(1_000).unwrap();
let session = issuer.verify_at(&token, 1_030).unwrap();

assert_eq!(session.expires_at, 29_800);
assert!(!issuer.verify_at("adss-user-session:v2.payload.signature", 1_030).is_some());
```

在 `api_contract.rs` 添加：正确 `POST /api/admin/session` 设置 `adss_management` Cookie；错误 token 返回 401；旧 Bearer 与普通用户 `adss_sso` Cookie 不能读取 `/api/admin/domains`；有效管理 Cookie 可以读取。

- [ ] **步骤：运行 Center 会话测试，确认新增断言失败**

运行：

```text
cargo test -p adss-center management_session
cargo test -p adss-center --test api_contract admin_session
```

预期：失败，原因是 `ManagementSessionIssuer`、`/api/admin/session` 和管理 Cookie 尚不存在。

- [ ] **步骤：实现独立管理会话与 CSRF nonce**

在 `session.rs` 增加如下独立 payload 和发行器；实现复用现有 base64url、HMAC-SHA256 与 Unix 时间逻辑，但前缀、派生密钥和 payload 不得复用普通用户会话：

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ManagementSession {
    pub(crate) auth_time: u64,
    pub(crate) expires_at: u64,
    pub(crate) csrf_nonce: String,
}

#[derive(Clone)]
pub(crate) struct ManagementSessionIssuer {
    key: Vec<u8>,
    ttl: Duration,
}

impl ManagementSessionIssuer {
    pub(crate) fn from_management_token(token: &str) -> anyhow::Result<Self>;
    pub(crate) fn issue(&self) -> anyhow::Result<String>;
    pub(crate) fn verify(&self, token: &str) -> Option<ManagementSession>;
    pub(crate) fn ttl_seconds(&self) -> u64;
}
```

`from_management_token` 使用 HMAC-SHA256 将 `ADSS_MANAGEMENT_TOKEN` 派生为 `adss:management-session:v1` 用途密钥；`issue` 为 `csrf_nonce` 生成 32 个随机字节的 base64url 值；默认 TTL 为 8 小时。`AppState` 保留 `management_token` 仅供登录常量时间比较，并新增 `management_sessions`。

- [ ] **步骤：实现会话端点和保护层**

在 `management_session.rs` 注册以下端点：

```text
POST   /api/admin/session  {"token":"..."} -> Set-Cookie + {"csrf_token":"..."}
GET    /api/admin/session  Cookie -> {"csrf_token":"..."}
DELETE /api/admin/session  Cookie + X-ADSS-CSRF-Token -> 清除 Cookie
```

Cookie 名为 `adss_management`，属性固定 `HttpOnly`、`Secure`、`SameSite=Strict`、`Path=/api/admin`、`Max-Age=<session TTL>`；三端点均返回 `Cache-Control: no-store`。在 `routes.rs` 为 `admin::routes()` 和 `oauth_clients::routes()` 添加统一路由层：读取请求验证 Cookie，`POST`、`PATCH`、`PUT`、`DELETE` 还以常量时间比较 `x-adss-csrf-token` 与经验证 session 的 `csrf_nonce`。认证失败统一返回 401。

删除 `admin.rs::authorize_management` 和业务 handler 中的 `HeaderMap` 鉴权参数；`oauth_clients.rs` 同步移除该依赖。管理 session 三端点不置于保护层内，普通用户、OIDC、Connector 路由保持不变。

- [ ] **步骤：补齐管理会话契约测试并确认通过**

测试必须覆盖 Cookie 四个安全属性、`no-store`、管理 token 不出现在响应头和响应体、刷新恢复、退出清 Cookie、过期拒绝、无/错/跨会话 CSRF 拒绝，以及携带同会话 CSRF 的 POST、PATCH、PUT、DELETE 成功。将 `api_contract.rs` 和 `oidc_contract.rs` 的 `admin_json*` 辅助函数改为先建 session、再携带 Cookie 与 CSRF；不得在测试辅助函数中继续发送管理 Bearer。

运行：

```text
cargo test -p adss-center
```

预期：`adss-center` 单元测试和两个契约测试均通过。

### 任务：管理台 Cookie 会话迁移

**文件：**
- 修改：`center/web/app/composables/useAdminApi.ts`
- 修改：`center/web/app/components/admin/AdminShell.vue`
- 创建：`center/web/tests/admin-session.test.ts`
- 修改：`center/web/package.json`

- [ ] **步骤：编写前端会话契约测试**

新增 `admin-session.test.ts`，以 `useAdminApi.ts` 的纯请求构造辅助函数为测试对象。断言 `POST /api/admin/session` 仅携带一次输入 token，`GET /api/admin/session` 使用 `credentials: 'same-origin'`，非安全方法带 `x-adss-csrf-token`，GET 不带该头，401/403 清除内存会话且不重放写请求。断言源码不包含 `localStorage`、`adss.managementToken` 或 `authorization: Bearer`。

- [ ] **步骤：运行 Bun 测试，确认前端契约失败**

运行：

```text
cd center/web
bun test tests/admin-session.test.ts
```

预期：失败，原因是管理 token 仍读取 localStorage 且所有请求仍使用 Bearer。

- [ ] **步骤：替换 composable 的凭据模型**

将 `useAdminApi.ts` 的 `managementToken` 替换为 `csrfToken` 与 `authenticated`。实现以下调用：

```ts
async function restoreSession(): Promise<void>
async function authenticateToken(token: string, showSuccess?: boolean): Promise<void>
async function logout(): Promise<void>

function requestHeaders(init: RequestInit): Headers {
  const headers = new Headers(init.headers)
  if (!['GET', 'HEAD', 'OPTIONS'].includes(init.method ?? 'GET') && csrfToken.value) {
    headers.set('x-adss-csrf-token', csrfToken.value)
  }
  return headers
}
```

`authenticateToken` 调用 `POST /api/admin/session`，成功后立即清空输入 token；`restoreSession` 调用 `GET /api/admin/session`；`adminFetch` 一律传 `credentials: 'same-origin'`，不再发送 Authorization。`logout` 发送 `DELETE /api/admin/session`，仅在服务端成功后清空本地目录数据和内存 CSRF。

- [ ] **步骤：迁移管理壳层并验证前端**

`AdminShell.vue` 挂载时调用 `restoreSession()`；退出动作改为异步 `logout()`；登录输入框只绑定临时 `credentialDraft`，不得把值传入 state、URL 或浏览器持久化存储。`package.json` 增加：

```json
"test": "bun test"
```

运行：

```text
cd center/web
bun test
bun run typecheck
```

预期：全部 Bun 测试和类型检查通过。

### 任务：Connector GSS-API 配置边界

**文件：**
- 修改：`Cargo.toml`
- 修改：`connector/src/config.rs`
- 修改：`connector/.env.example`
- 修改：`connector/tests/http_client_contract.rs`

- [ ] **步骤：为仅允许 FQDN LDAP 389 写失败测试**

在 `http_client_contract.rs` 将真实模式合法配置改为：

```text
ADSS_LDAP_URL=ldap://dc01.rd.kim:389
```

新增表驱动断言，拒绝 `ldaps://dc01.rd.kim:636`、`ldap://192.168.2.6:389`、`ldap://dc01.rd.kim:1389`、带路径或查询参数的 URL，以及任一 `ADSS_LDAP_BIND_*` 或 `ADSS_LDAP_ACCEPT_INVALID_CERTS` 环境变量。

- [ ] **步骤：运行 Connector 配置测试，确认失败**

运行：

```text
cargo test -p adss-connector --test http_client_contract connector_process_config
```

预期：失败，当前配置仍接受 LDAPS/IP/旧 Simple Bind 字段。

- [ ] **步骤：实现 GSS-API 配置模型**

将 workspace `ldap3` 依赖改为：

```toml
ldap3 = { version = "0.12.1", default-features = false, features = ["gssapi"] }
```

将 `LdapDirectoryConfig` 收敛为 `url`、`server_fqdn`、`adopt_existing_users_by_username`。`from_env` 使用 `url::Url` 解析并要求 scheme 为 `ldap`、端口为 389、host 为非 IP FQDN、path 为 `/`、无用户名、无查询和片段；若环境中出现已移除的 LDAP bind/TLS 变量则明确报错。`.env.example` 删除 bind DN、密码和证书选项。

- [ ] **步骤：运行 Connector 配置测试，确认通过**

运行：

```text
cargo test -p adss-connector --test http_client_contract
```

预期：合法 FQDN 配置通过，所有降级路径被拒绝，Debug 输出不泄露 Connector key。

### 任务：批次 Kerberos LDAP 会话

**文件：**
- 修改：`connector/src/directory/mod.rs`
- 修改：`connector/src/directory/dry_run.rs`
- 修改：`connector/src/directory/ldap.rs`
- 修改：`connector/src/runtime.rs`
- 修改：`connector/src/lib.rs`
- 测试：`connector/tests/execution_contract.rs`
- 测试：`connector/tests/runtime_contract.rs`

- [ ] **步骤：为批次连接复用写失败测试**

在 `execution_contract.rs` 的记录型 fake 中记录 `open_batch` 次数。对含两个目录操作和两个凭据项的输入，断言每个非空通道只打开一次，且仍按顺序执行、首错后跳过余项、单项 timeout 后停止。`runtime_contract.rs` 额外断言打开 session 失败时发送失败 confirm 且不写本地 revision；空批次不打开 LDAP session。

- [ ] **步骤：运行 Connector 执行测试，确认失败**

运行：

```text
cargo test -p adss-connector --test execution_contract
cargo test -p adss-connector --test runtime_contract
```

预期：失败，当前 `DirectoryClient` 对每个 `apply` 和 `set_password` 都独立 bind。

- [ ] **步骤：引入批次 session 抽象并迁移 dry-run**

将目录执行边界改为可打开、可变借用的批次 session：

```rust
#[async_trait]
pub trait DirectoryBatchSession {
    async fn apply(&mut self, operation: &DirectoryOperation, context: &DirectoryExecutionContext)
        -> anyhow::Result<()>;
    async fn set_password(&mut self, credential: &CredentialEntry, context: &DirectoryExecutionContext)
        -> anyhow::Result<()>;
}

#[async_trait]
pub trait DirectoryClient {
    type Batch: DirectoryBatchSession + Send;
    async fn open_batch(&self) -> anyhow::Result<Self::Batch>;
}
```

执行器在非空通道开头调用一次 `open_batch()`，对 batch 内每项继续使用现有 `tokio::time::timeout`。Dry-run 实现无状态 `DryRunDirectoryBatch`，从而保持 dry-run 不访问网络。

- [ ] **步骤：实现 LDAP GSS-API batch 并接入 runtime**

`LdapDirectoryClient::open_batch` 建立 `LdapConnAsync` 后调用：

```rust
ldap.sasl_gssapi_bind(&self.config.server_fqdn).await?.success()?;
```

返回持有 `Ldap` 的 `LdapDirectoryBatch`；将现有 `ensure_*` 与密码设置辅助方法移到该 batch 上，彻底删除 `simple_bind`。`ldap3` 在无 TLS 连接中无法协商 Kerberos 保密层时会返回错误，runtime 将其作为本通道执行失败处理。批次结束时调用 `unbind`，即使 unbind 失败也不得覆盖已发生的执行错误。

- [ ] **步骤：运行 Connector 回归测试**

运行：

```text
cargo test -p adss-connector
```

预期：Connector 单元和契约测试通过。真实 GSS-API 不在单元测试中伪造。

### 任务：Windows 服务与部署契约

**文件：**
- 修改：`deploy/connector/install-service.ps1`
- 修改：`scripts/test-connector-service-scripts.ps1`
- 修改：`deploy/connector/README.md`
- 修改：`docs/guide/deployment.md`
- 修改：`docs/guide/security.md`
- 修改：`docs/reference/security-boundary.md`
- 修改：`docs/reference/connector-sync-protocol.md`

- [ ] **步骤：为 NetworkService 和 ACL 写失败契约**

在 `test-connector-service-scripts.ps1` 将身份断言替换为 SID `S-1-5-20` 和 `NT AUTHORITY\NetworkService`。新增断言：安装脚本对 `.env` 使用 `icacls /inheritance:r`，显式授予且只授予 `SYSTEM`、`Administrators`、`NetworkService`；脚本不包含 `ADSS_LDAP_BIND_DN`、`ADSS_LDAP_BIND_PASSWORD` 或 `LocalService`。

- [ ] **步骤：运行服务脚本契约，确认失败**

运行：

```powershell
pwsh -NoProfile -File scripts/test-connector-service-scripts.ps1
```

预期：失败，当前脚本仍使用 LocalService 且未禁用 `.env` ACL 继承。

- [ ] **步骤：实现 NetworkService 安装与文档前提**

安装脚本以 `NT AUTHORITY\NetworkService` 创建服务，使用 SID `*S-1-5-20` 授权运行目录、state 和日志。对 `.env` 先删除继承，再显式设置三类主体的权限；Connector key 仍是该文件唯一的同步秘密。部署文档以如下配置为唯一真实 LDAP 示例：

```text
ADSS_CONNECTOR_DRY_RUN=0
ADSS_LDAP_URL=ldap://dc01.rd.kim:389
```

文档明确：Connector 主机必须加入 `rd.kim` 域，域管理员将镜像根和隔离 OU 的最小权限委派给 `RD\<CONNECTOR-HOST>$`；禁止以 `superuser`、LocalService 或手工保存 LDAP 密码运行。

- [ ] **步骤：运行服务脚本与文档契约**

运行：

```powershell
pwsh -NoProfile -File scripts/test-connector-service-scripts.ps1
```

运行：

```text
rg -n 'ADSS_LDAP_BIND|LDAPS|LocalService' connector/.env.example deploy/connector docs/guide docs/reference
```

预期：PowerShell 契约通过；搜索结果只允许出现在历史评审报告，不得出现在有效部署说明或示例配置。

### 任务：用户与初始凭据原子创建

**文件：**
- 修改：`crates/store/src/models.rs`
- 修改：`crates/store/src/lib.rs`
- 修改：`crates/store/src/repository.rs`
- 修改：`center/src/routes/admin.rs`
- 测试：`crates/store/src/repository.rs`
- 测试：`center/tests/api_contract.rs`

- [ ] **步骤：为原子创建写失败测试**

新增 repository 测试，以 SQLite trigger 让 `user_credential` 插入返回错误；调用新方法后断言 `get_user(employee_id)` 和 `get_credential_record(employee_id)` 都为空，目录和凭据 revision 均未推进。API 契约额外断言 `POST /api/admin/users` 成功仍同时返回用户、目录 revision 和凭据 revision。

- [ ] **步骤：运行 Store 测试，确认失败**

运行：

```text
cargo test --manifest-path crates/store/Cargo.toml create_user
```

预期：失败，当前路由先单独 `upsert_directory`，再在独立事务中写密码。

- [ ] **步骤：实现单一持久化事务**

在 models 中定义：

```rust
pub struct UserCreateInput {
    pub directory: UserDirectoryPatch,
    pub credential: UserCredentialInput,
}
```

在 `Repository::create_user_with_initial_credential` 的同一 SeaORM transaction 中分配目录 revision、插入用户、分配凭据 revision、插入凭据并 commit。失败路径一律 rollback；成功返回 `(User, directory_revision, credential_revision)`。`admin.rs::create_user` 在进入 repository 前完成密码 seal/hash，随后只调用该方法。

- [ ] **步骤：运行原子创建回归**

运行：

```text
cargo test --manifest-path crates/store/Cargo.toml
cargo test -p adss-center --test api_contract create_user
```

预期：trigger 注入失败无残留，正常 API 创建维持原响应契约。

### 任务：有界目录批次与 OU 上下文

**文件：**
- 修改：`crates/protocol/src/lib.rs`
- 修改：`crates/protocol/tests/sync_contract.rs`
- 修改：`crates/store/src/repository.rs`
- 修改：`center/src/routes.rs`
- 修改：`connector/src/directory/mod.rs`
- 修改：`connector/tests/runtime_contract.rs`
- 测试：`crates/store/src/repository.rs`
- 测试：`center/tests/api_contract.rs`

- [ ] **步骤：为目录 revision 分页写失败测试**

建立至少三个不同 directory revision，调用 `list_directory_changed_after(..., limit = 2)` 并断言：返回前两个 revision 的变更对象、`batch_revision` 等于第二个 revision、`has_more` 为 true；确认该 revision 后下次请求只返回后续变更。测试同时断言响应中的 OU DN 映射只包含本批目录操作所引用的 OU，不包含全量 OU 树。

- [ ] **步骤：运行 protocol、Store 与 Center 分页测试，确认失败**

运行：

```text
cargo test --manifest-path crates/protocol/Cargo.toml directory_batch
cargo test --manifest-path crates/store/Cargo.toml list_directory_changed_after
cargo test -p adss-center --test api_contract connector_sync
```

预期：失败，当前 `_limit` 未使用，`has_more` 固定 false，并在变更发生时返回全量 OU。

- [ ] **步骤：让协议传递最小 OU DN 上下文**

在 `DirectoryBatch` 增加：

```rust
pub organizational_unit_dns: BTreeMap<String, String>,
```

`DirectoryExecutionContext::try_from_batch` 直接复制该映射，不再用 `organizational_units` 递归计算全部 DN。更新 protocol 序列化测试和 Connector fixture，使每个被操作的 OU、用户或组都能通过其 OU ID 找到 DN。

- [ ] **步骤：按 revision 选择对象并计算必需 DN**

Store 先收集 OU、用户、组在 threshold 之后的去重 revision，按 `limit.max(1)` 选出 `batch_revision` 和 `has_more`；三个对象查询均限制 `changed_revision <= batch_revision`。为本批对象涉及的 OU 沿父链读取当前 OU，计算其 DN 并只放入 `organizational_unit_dns`。当无变更时返回空对象、空 DN 映射、`batch_revision = server_revision`、`has_more = false`。

Center 继续传递 `state.batch_limit`，Connector 继续只在批次成功后 confirm `batch_revision`，从而下一轮使用该 revision 拉取后续页。

- [ ] **步骤：运行分页回归**

运行：

```text
cargo test --manifest-path crates/protocol/Cargo.toml
cargo test --manifest-path crates/store/Cargo.toml
cargo test -p adss-center --test api_contract
cargo test -p adss-connector --test runtime_contract
```

预期：目录数据按 revision 页推进，OU 上下文有界且 Connector 的现有执行和 confirm 语义保持通过。

### 任务：质量入口与全量验证

**文件：**
- 修改：`center/web/package.json`
- 修改：`docs/reference/README.md`
- 修改：`docs/guide/overview.md`

- [ ] **步骤：补齐本地检查入口和文档测试**

`package.json` 保留 `test: "bun test"`，文档将前端入口列为 `bun test`、`bun run typecheck`、`bun run build`；Rust 文档列出 workspace 与独立 protocol/store manifest 的 fmt、test、clippy 命令。`overview.md` 将已交付 OIDC Provider 说明为受限支持，继续明确 SAML 和 AD FS 不在范围。

- [ ] **步骤：执行最终静态验证**

运行：

```text
cargo fmt --all
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --manifest-path crates/protocol/Cargo.toml
cargo test --manifest-path crates/protocol/Cargo.toml
cargo clippy --manifest-path crates/protocol/Cargo.toml --all-targets --all-features -- -D warnings
cargo fmt --manifest-path crates/store/Cargo.toml
cargo test --manifest-path crates/store/Cargo.toml
cargo clippy --manifest-path crates/store/Cargo.toml --all-targets --all-features -- -D warnings
cd center/web && bun test && bun run typecheck && bun run build
pwsh -NoProfile -File scripts/test-docker-contract.ps1
pwsh -NoProfile -File scripts/test-connector-service-scripts.ps1
pwsh -NoProfile -File scripts/test-release-contract.ps1
git diff --check
```

预期：所有静态检查通过。若 Nuxt/Nitro 仍输出警告，记录精确警告和锁定依赖版本后再判定构建验收。

- [ ] **步骤：执行真实域环境验收清单**

在域成员 Connector 主机上使用 `NetworkService` 安装服务；确认 `dc01.rd.kim` 可解析且 GSS-API bind 成功；在 AD 中确认实际网络身份为 `RD\<CONNECTOR-HOST>$`；仅向镜像根和隔离 OU 委派权限；验证 OU、用户、组、成员、禁用、隔离移动和 Reset Password；验证无委派范围外写入权限；重启服务后确认 revision 不回退。不得以单元测试替代这些结论。
