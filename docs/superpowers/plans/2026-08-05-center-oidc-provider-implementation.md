# Center OIDC 统一登录实施计划

> **供 agentic workers 使用：** 必须使用 `superpowers:subagent-driven-development`（推荐）或 `superpowers:executing-plans`，按复选框逐项实施并在每个任务后复核。

**目标：** 在现有 Center 中实现受限的 OIDC Authorization Code + PKCE 登录服务，并在现有管理端维护 Web 与桌面客户端。

**架构：** `crates/store` 保存 OAuth 客户端和一次性授权码；`center/src/oidc` 负责协议校验、RS256 JWT、JWKS、Discovery、UserInfo 和授权流程；现有登录增加无状态 HMAC Cookie；Nuxt 增加客户端管理与逐次确认页面。现有用户 Bearer token、管理 token 和 Connector 认证边界保持不变。

**技术栈：** Rust 2024、Axum 0.8、SeaORM 2.0 RC、SQLite/PostgreSQL、`jsonwebtoken` 11、RustCrypto RSA/HMAC/SHA-256、`url`、Nuxt 4、Vue 3、Bun。

---

## 文件结构

新增或调整的文件职责如下：

- `crates/store/src/models.rs`：公开 OAuth 客户端和授权码持久化类型。
- `crates/store/src/entities.rs`：`oauth_clients` 与 `oauth_authorization_codes` SeaORM entity。
- `crates/store/src/oauth.rs`：OAuth Repository 方法，避免继续扩大通用 `repository.rs`。
- `crates/store/src/repository.rs`：建表，并把数据库连接以 crate 内可见方式提供给 OAuth 模块。
- `crates/store/tests/oauth_repository_contract.rs`：客户端 CRUD、过期清理和授权码原子消费契约。
- `center/src/session.rs`：HMAC-SHA256 会话 token、登录时间和 Cookie 数据。
- `center/src/oidc/config.rs`：issuer、RSA 私钥和固定 TTL 配置。
- `center/src/oidc/crypto.rs`：随机值、摘要、PKCE、CSRF、RS256 JWT 和 JWKS。
- `center/src/oidc/validation.rs`：scope、客户端类型和 redirect URI 校验。
- `center/src/oidc/routes.rs`：Discovery、authorize、token、UserInfo、JWKS 和授权上下文路由。
- `center/src/oidc/mod.rs`：模块出口和 `OidcService` 聚合。
- `center/src/routes/oauth_clients.rs`：管理端 OAuth 客户端 CRUD 和 secret 重新生成。
- `center/src/routes.rs`、`center/src/state.rs`、`center/src/lib.rs`：挂载路由、Cookie 登录与状态依赖。
- `center/tests/oidc_contract.rs`：完整 Web/Desktop 授权链路和错误矩阵。
- `center/tests/fixtures/oidc-private-key.pem`：只用于测试的固定 RSA 私钥。
- `center/web/app/pages/admin/clients.vue`：客户端管理页面。
- `center/web/app/components/oauth/OAuthClientTable.vue`：客户端列表。
- `center/web/app/components/oauth/OAuthClientEditor.vue`：创建和编辑表单。
- `center/web/app/pages/authorize.vue`：每次登录确认页面。
- `center/web/app/pages/login.vue`、`center/web/app/composables/useUserApi.ts`：授权继续地址和 Cookie 注销。
- `center/web/app/types/admin.ts`、`center/web/app/types/oidc.ts`：前端契约类型。
- `center/web/app/assets/css/main.css`：客户端管理和确认页的响应式样式。
- `center/.env.example`、`deploy/center/*` 和现有用户/参考文档：issuer、私钥挂载、端点和安全说明。

## OAuth 持久化模型

**文件：**

- 修改：`crates/store/src/lib.rs`
- 修改：`crates/store/src/models.rs`
- 修改：`crates/store/src/entities.rs`
- 修改：`crates/store/src/repository.rs`
- 新增：`crates/store/src/oauth.rs`
- 新增：`crates/store/tests/oauth_repository_contract.rs`

- [ ] **写入客户端 Repository 失败测试**

在新测试文件中定义 Web 和 Desktop 记录，验证创建、按 ID 查询、按 ID 排序列表、更新、冲突和删除：

```rust
#[tokio::test]
async fn oauth_client_crud_preserves_structured_configuration() {
    let repository = sqlite_repository().await;
    let created = repository
        .create_oauth_client(OAuthClientRecord {
            client_id: "client-web".into(),
            name: "Web Portal".into(),
            client_type: OAuthClientType::Web,
            client_secret_hash: Some("sha256:web".into()),
            redirect_uris: vec!["https://portal.example.com/callback".into()],
            allowed_scopes: vec!["openid".into(), "profile".into()],
            enabled: true,
        })
        .await
        .unwrap();
    assert!(created.is_some());
    assert!(repository.create_oauth_client(created.unwrap()).await.unwrap().is_none());
    assert_eq!(repository.get_oauth_client("client-web").await.unwrap().unwrap().name, "Web Portal");
}
```

- [ ] **运行测试并确认因类型和方法不存在而失败**

```text
cargo test --manifest-path crates/store/Cargo.toml --test oauth_repository_contract oauth_client_crud_preserves_structured_configuration
```

预期：编译失败，提示 `OAuthClientRecord`、`OAuthClientType` 或 Repository 方法不存在。

- [ ] **定义公开模型和数据库实体**

在 `models.rs` 定义：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OAuthClientType { Web, Desktop }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthClientRecord {
    pub client_id: String,
    pub name: String,
    pub client_type: OAuthClientType,
    pub client_secret_hash: Option<String>,
    pub redirect_uris: Vec<String>,
    pub allowed_scopes: Vec<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorizationCodeRecord {
    pub code_hash: String,
    pub client_id: String,
    pub employee_id: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    pub nonce: String,
    pub code_challenge: String,
    pub auth_time: i64,
    pub expires_at: i64,
}
```

为枚举提供严格的 `web`/`desktop` 存储转换。JSON 数组使用 `serde_json` 编解码，解析失败向上返回错误，不能静默回退为空列表。

在 `entities.rs` 增加两个 entity，数据库列使用 `TEXT` 保存 JSON 数组，时间使用 `BIGINT` Unix 秒。

- [ ] **建立 schema 并实现客户端 CRUD**

`initialize_schema` 增加：

```sql
CREATE TABLE IF NOT EXISTS oauth_clients (
    client_id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    client_type TEXT NOT NULL,
    client_secret_hash TEXT NULL,
    redirect_uris TEXT NOT NULL,
    allowed_scopes TEXT NOT NULL,
    enabled BOOLEAN NOT NULL
)
```

在 `oauth.rs` 为 `Repository` 实现 `list_oauth_clients`、`get_oauth_client`、`create_oauth_client`、`update_oauth_client` 和 `delete_oauth_client`。`create` 把唯一约束映射成 `Ok(None)`，`update` 和 `delete` 用 `Option`/`bool` 区分不存在。

- [ ] **运行客户端 Repository 测试**

```text
cargo test --manifest-path crates/store/Cargo.toml --test oauth_repository_contract oauth_client
```

预期：客户端 CRUD、JSON 往返和唯一约束测试通过。

- [ ] **提交持久化客户端模型**

```text
git add crates/store/src/lib.rs crates/store/src/models.rs crates/store/src/entities.rs crates/store/src/repository.rs crates/store/src/oauth.rs crates/store/tests/oauth_repository_contract.rs
git commit -m "增加 OAuth 客户端持久化"
```

## 一次性授权码

**文件：**

- 修改：`crates/store/src/oauth.rs`
- 修改：`crates/store/src/repository.rs`
- 修改：`crates/store/tests/oauth_repository_contract.rs`

- [ ] **写入授权码原子消费失败测试**

测试保存记录、首次消费成功、再次消费为空、过期记录不返回，并以 `tokio::join!` 验证并发调用只有一次得到记录：

```rust
let first_repository = repository.clone();
let second_repository = repository.clone();
let (first, second) = tokio::join!(
    first_repository.consume_authorization_code("sha256:code", now),
    second_repository.consume_authorization_code("sha256:code", now),
);
assert_eq!(usize::from(first.unwrap().is_some()) + usize::from(second.unwrap().is_some()), 1);
```

- [ ] **运行测试并确认方法缺失**

```text
cargo test --manifest-path crates/store/Cargo.toml --test oauth_repository_contract authorization_code
```

预期：编译失败，提示授权码 Repository 方法不存在。

- [ ] **建表并实现原子消费**

新增表：

```sql
CREATE TABLE IF NOT EXISTS oauth_authorization_codes (
    code_hash TEXT PRIMARY KEY NOT NULL,
    client_id TEXT NOT NULL,
    employee_id TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    scopes TEXT NOT NULL,
    nonce TEXT NOT NULL,
    code_challenge TEXT NOT NULL,
    auth_time BIGINT NOT NULL,
    expires_at BIGINT NOT NULL
)
```

实现：

```rust
pub async fn store_authorization_code(&self, record: AuthorizationCodeRecord) -> anyhow::Result<()>;
pub async fn consume_authorization_code(&self, code_hash: &str, now: i64) -> anyhow::Result<Option<AuthorizationCodeRecord>>;
pub async fn delete_expired_authorization_codes(&self, now: i64, limit: u64) -> anyhow::Result<u64>;
```

消费使用单个数据库事务和 SeaORM `delete_by_id(...).exec_with_returning(...)`，先获得被删除行，再判断到期时间；无论记录是否过期都不恢复。有限清理先按到期时间查询最多 `limit` 个主键，再批量删除，不能执行无上限全表清理。

- [ ] **运行授权码和完整 Store 测试**

```text
cargo test --manifest-path crates/store/Cargo.toml --test oauth_repository_contract authorization_code
cargo test --manifest-path crates/store/Cargo.toml
```

预期：原子消费和现有 Repository 契约全部通过。

- [ ] **提交授权码存储**

```text
git add crates/store/src/oauth.rs crates/store/src/repository.rs crates/store/tests/oauth_repository_contract.rs
git commit -m "增加一次性授权码存储"
```

## 密码学与无状态会话

**文件：**

- 修改：`Cargo.toml`
- 修改：`center/Cargo.toml`
- 修改：`Cargo.lock`
- 修改：`center/src/session.rs`
- 新增：`center/src/oidc/mod.rs`
- 新增：`center/src/oidc/crypto.rs`
- 新增：`center/src/oidc/validation.rs`

- [ ] **加入经过核验的依赖版本**

工作区依赖使用：

```toml
axum-extra = { version = "0.12.6", features = ["cookie"] }
base64 = "0.22.1"
hmac = "0.12.1"
jsonwebtoken = { version = "11.0.0", features = ["rust_crypto"] }
rand = "0.9.4"
rsa = { version = "0.9.10", features = ["pem"] }
url = "2.5.8"
```

Center 从 workspace 引用这些依赖。`jsonwebtoken` 只启用 `rust_crypto` 后端，不同时启用 `aws_lc_rs`。

- [ ] **写入会话、PKCE、随机值和 redirect URI 单元失败测试**

测试必须覆盖：HMAC token 篡改、登录时间/到期时间、RFC 7636 S256 向量、随机值长度、Web 精确 URI、Desktop loopback 只变端口、拒绝 `localhost`/fragment/userinfo。

```rust
assert_eq!(
    pkce_s256("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
    "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
);
```

- [ ] **运行单元测试并确认失败**

```text
cargo test -p adss-center session::tests
cargo test -p adss-center oidc::crypto::tests
cargo test -p adss-center oidc::validation::tests
```

预期：新函数或模块尚不存在而编译失败。

- [ ] **把现有会话改成标准 HMAC-SHA256**

`UserSessionIssuer::issue` 返回带 `employee_id`、`auth_time` 和 `expires_at` 的 v2 token，`verify` 返回：

```rust
pub(crate) struct UserSession {
    pub(crate) employee_id: String,
    pub(crate) auth_time: u64,
    pub(crate) expires_at: u64,
}
```

使用 `Hmac<Sha256>::new_from_slice`、`update` 和 `verify_slice`；payload 使用 URL-safe no-pad base64 编码的 JSON。删除普通 SHA-256 拼接密钥签名逻辑，不使用自定义常量时间 MAC 比较替代 `verify_slice`。

- [ ] **实现通用 OIDC 安全原语**

`crypto.rs` 提供 `random_urlsafe(bytes)`、`sha256_token`、`pkce_s256`、CSRF 签发/验证；`validation.rs` 提供固定 scope 解析、Web URI 精确匹配和 Desktop loopback 匹配。所有 URI 都先用 `url::Url` 解析，再比较结构字段。

验证边界固定为：client ID 最长 128 字符、客户端名称 1 至 100 字符、每个客户端最多 10 个 redirect URI、单个 URI 最长 2048 字符、scope 字符串最长 256 字符且最多 4 个不重复固定 scope、state 最长 512 字符、nonce 1 至 256 字符、S256 challenge 恰好 43 个 base64url 字符、verifier 43 至 128 个 RFC 7636 unreserved 字符。`response_mode` 只允许缺省或 `query`。OIDC query 和表单路由使用 16 KiB body 上限，超过限制返回受控错误。

- [ ] **运行单元测试和现有 Center 契约**

```text
cargo test -p adss-center session::tests
cargo test -p adss-center oidc::crypto::tests
cargo test -p adss-center oidc::validation::tests
cargo test -p adss-center --test api_contract
```

预期：新单元测试通过；现有登录 token 仍能访问 `/api/me`。

- [ ] **提交密码学基础能力**

```text
git add Cargo.toml center/Cargo.toml Cargo.lock center/src/session.rs center/src/oidc/mod.rs center/src/oidc/crypto.rs center/src/oidc/validation.rs
git commit -m "增加 OIDC 安全基础能力"
```

## OIDC 配置、JWT 与 JWKS

**文件：**

- 新增：`center/src/oidc/config.rs`
- 修改：`center/src/oidc/crypto.rs`
- 修改：`center/src/oidc/mod.rs`
- 修改：`center/src/state.rs`
- 新增：`center/tests/fixtures/oidc-private-key.pem`

- [ ] **写入配置和 JWT 失败测试**

测试固定 issuer 与测试 RSA 私钥，验证非法 HTTP issuer、带 query/fragment 的 issuer、无效 PEM 均失败；签发 ID Token 后用 JWKS 的 `n`/`e` 构造 `DecodingKey` 验证 `RS256`、`kid`、`iss`、`aud`、`nonce` 和五分钟 TTL。

```rust
assert_eq!(claims.iss, "https://center.example.test");
assert_eq!(claims.aud, "client-web");
assert_eq!(claims.exp - claims.iat, 300);
assert_eq!(header.alg, Algorithm::RS256);
assert_eq!(header.kid.as_deref(), Some(service.key_id()));
```

- [ ] **运行测试并确认配置类型缺失**

```text
cargo test -p adss-center oidc::config::tests
cargo test -p adss-center oidc::crypto::tests::id_token
```

- [ ] **实现配置和 RS256 服务**

定义：

```rust
pub(crate) struct OidcConfig {
    pub(crate) issuer: Url,
    pub(crate) private_key_pem: Vec<u8>,
    pub(crate) allow_insecure_web_loopback_redirects: bool,
    pub(crate) authorization_code_ttl: Duration,
    pub(crate) token_ttl: Duration,
}

pub(crate) struct OidcService {
    pub(crate) config: OidcConfig,
    pub(crate) encoding_key: EncodingKey,
    pub(crate) decoding_key: DecodingKey,
    pub(crate) jwks: JwkSetResponse,
}
```

`from_env` 读取 `ADSS_OIDC_ISSUER`、`ADSS_OIDC_PRIVATE_KEY_FILE` 和默认关闭的 `ADSS_OIDC_ALLOW_INSECURE_WEB_LOOPBACK_REDIRECTS`。issuer 必须是 HTTPS origin，不能带 userinfo、query 或 fragment；path 只允许 `/`。开发开关只放宽 Web client 的 HTTP loopback redirect URI，不能放宽非 loopback host 或 issuer。使用 `rsa::RsaPrivateKey` 解析 PKCS#8/PKCS#1 PEM，派生公钥 `n`/`e`，URL-safe no-pad 编码；`kid` 为公钥参数摘要。JWT 验证时钟偏差固定为 30 秒，不能从请求参数扩大。

为集成测试提供显式接收 issuer 和 PEM 的构造函数，测试私钥只放在 `center/tests/fixtures`，生产构造函数不能回退到测试密钥或临时生成密钥。

- [ ] **将 OIDC 服务注入 AppState**

`AppState::from_env` 必须加载并验证 OIDC 配置；测试构造函数显式接收测试 `OidcService`。更新现有 API 测试 helper，使所有测试使用固定测试 issuer 和 fixture key，避免生产状态使用 `Option<OidcService>`。

- [ ] **运行配置、JWT 和 API 测试**

```text
cargo test -p adss-center oidc::
cargo test -p adss-center --test api_contract
```

- [ ] **提交 OIDC 配置和签名服务**

```text
git add center/src/oidc/config.rs center/src/oidc/crypto.rs center/src/oidc/mod.rs center/src/state.rs center/tests/fixtures/oidc-private-key.pem center/tests/api_contract.rs
git commit -m "增加 OIDC 签名和配置"
```

## OAuth 客户端管理 API

**文件：**

- 新增：`center/src/routes/oauth_clients.rs`
- 修改：`center/src/routes.rs`
- 修改：`center/src/routes/admin.rs`
- 新增：`center/tests/oidc_contract.rs`

- [ ] **写入管理 API 失败测试**

覆盖缺少管理 token、创建 Web/Desktop、一次性 secret、列表不返回 secret、编辑保持 `client_id`/类型、Web secret 重新生成、Desktop 禁止生成 secret、停用和删除。

```rust
assert_eq!(created["client"]["client_type"], "web");
assert!(created["client_secret"].as_str().unwrap().len() >= 43);
assert!(listed["clients"][0].get("client_secret").is_none());
assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
```

- [ ] **运行测试并确认路由为 404**

```text
cargo test -p adss-center --test oidc_contract admin_oauth_client
```

预期：请求 `/api/admin/oauth-clients` 返回 404。

- [ ] **实现管理路由和输入校验**

提供：

```text
GET    /api/admin/oauth-clients
POST   /api/admin/oauth-clients
PATCH  /api/admin/oauth-clients/{client_id}
DELETE /api/admin/oauth-clients/{client_id}
POST   /api/admin/oauth-clients/{client_id}/secret
```

创建请求包含 `name`、`client_type`、`redirect_uris`、`allowed_scopes`、`enabled`。编辑不允许修改 `client_id` 或 `client_type`。服务端生成 `client_` 前缀的随机 ID；Web secret 使用 32 随机字节，保存 `sha256:` 摘要；Desktop 的 secret 始终为空。创建和重新生成响应返回 `client_secret` 并设置 `Cache-Control: no-store`。

所有 redirect URI 和 scope 调用 `oidc::validation`，不能在管理路由复制一套规则。把 `authorize_management` 调整为 `pub(super)` 供同级模块复用。

- [ ] **运行管理 API 与现有管理契约**

```text
cargo test -p adss-center --test oidc_contract admin_oauth_client
cargo test -p adss-center --test api_contract admin
```

- [ ] **提交客户端管理 API**

```text
git add center/src/routes/oauth_clients.rs center/src/routes.rs center/src/routes/admin.rs center/tests/oidc_contract.rs
git commit -m "增加 OAuth 客户端管理接口"
```

## Discovery、JWKS 与 UserInfo

**文件：**

- 新增：`center/src/oidc/routes.rs`
- 修改：`center/src/oidc/mod.rs`
- 修改：`center/src/routes.rs`
- 修改：`center/tests/oidc_contract.rs`

- [ ] **写入元数据和 UserInfo 失败测试**

验证 Discovery 只声明设计范围、JWKS 不含私钥字段、无 token/错误 audience/过期 token/停用用户/停用客户端均拒绝，scope 控制返回字段。

```rust
assert_eq!(metadata["response_types_supported"], json!(["code"]));
assert_eq!(metadata["grant_types_supported"], json!(["authorization_code"]));
assert_eq!(metadata["code_challenge_methods_supported"], json!(["S256"]));
assert!(jwks["keys"][0].get("d").is_none());
assert_eq!(userinfo["sub"], "1001");
assert!(userinfo.get("email").is_none());
```

- [ ] **运行测试并确认公开端点缺失**

```text
cargo test -p adss-center --test oidc_contract discovery
cargo test -p adss-center --test oidc_contract jwks
cargo test -p adss-center --test oidc_contract userinfo
```

- [ ] **实现公开只读端点**

Discovery 从配置 issuer 构造端点，不能读取请求 Host。JWKS 返回固定 `RS256` 公钥。UserInfo 只接受 `Authorization: Bearer` OIDC access token，验证算法、签名、`kid`、issuer、UserInfo audience、到期时间、客户端和用户状态，再按 scope 读取并返回当前用户资料。

Discovery/JWKS 可使用短期公共缓存；UserInfo 始终设置 `Cache-Control: no-store`。错误响应不进入现有 `ApiError` 空响应，而使用 OIDC JSON 错误类型。

- [ ] **运行只读端点和完整 Center 测试**

```text
cargo test -p adss-center --test oidc_contract discovery
cargo test -p adss-center --test oidc_contract jwks
cargo test -p adss-center --test oidc_contract userinfo
cargo test -p adss-center
```

- [ ] **提交 OIDC 元数据和 UserInfo**

```text
git add center/src/oidc/routes.rs center/src/oidc/mod.rs center/src/routes.rs center/tests/oidc_contract.rs
git commit -m "增加 OIDC 元数据和用户信息端点"
```

## SSO Cookie 与授权确认

**文件：**

- 修改：`center/src/routes.rs`
- 修改：`center/src/session.rs`
- 修改：`center/src/oidc/routes.rs`
- 修改：`center/tests/oidc_contract.rs`

- [ ] **写入登录 Cookie 和授权错误失败测试**

验证登录响应同时保留 JSON token 并设置 `adss_sso` Cookie；Cookie 包含 `HttpOnly`、`Secure`、`SameSite=Lax`、`Path=/`。验证未知客户端/非法 redirect URI 使用本地错误，可信 redirect URI 的 scope 错误带原始 state 返回，`prompt=none` 返回 `interaction_required`。

- [ ] **写入授权上下文和确认失败测试**

完整检查：未登录授权跳转到内部登录页；有效 Cookie 跳转 `/authorize`；`GET /api/oauth2/authorize/context` 返回客户端、用户、字段和 CSRF；确认产生授权码；取消返回 `access_denied`；篡改参数或 CSRF 失败；切换账号/注销清除 Cookie。

```rust
assert!(location.starts_with("/login?continue="));
assert_eq!(context["client_name"], "Web Portal");
assert_eq!(context["claims"]["preferred_username"], "zhangsan");
assert!(decision_location.starts_with("https://portal.example.com/callback?code="));
```

- [ ] **运行测试并确认缺少 Cookie 与授权流程**

```text
cargo test -p adss-center --test oidc_contract authorization
```

- [ ] **实现登录 Cookie 和注销**

`POST /api/auth/login` 返回 `(CookieJar, Json<UserLoginResponse>)`，把同一个 v2 会话 token 写入名为 `adss_sso` 的 Cookie；现有 JSON token 继续用于 `/api/me`。Cookie 明确设置 `HttpOnly`、`Secure`、`SameSite=Lax`、`Path=/` 和与现有 `ADSS_USER_SESSION_TTL_SECONDS` 一致的 `Max-Age`。`POST /api/auth/logout` 使用相同 name/path/security 属性生成过期 Cookie。`authorize_user_session` 改为读取 `UserSession.employee_id`，保持 Bearer 入口不读取 Cookie。

- [ ] **实现 authorize GET、上下文和确认 POST**

`GET /oauth2/authorize` 先验证完整请求。无 Cookie 时只生成内部 `/login?continue=/oauth2/authorize?...`；有 Cookie 时进入 `/authorize?...`。内部上下文端点重新验证请求和用户，并签发绑定 `employee_id`、授权请求摘要与短期到期时间的 CSRF token。

`POST /oauth2/authorize` 使用表单接收原始授权字段、`decision` 和 CSRF。服务端重新执行所有校验；确认时清理少量过期码、生成随机码并只保存摘要；取消时返回标准错误。重定向使用 `url::Url::query_pairs_mut`，不能字符串拼接。

- [ ] **运行授权流程与现有登录测试**

```text
cargo test -p adss-center --test oidc_contract authorization
cargo test -p adss-center --test api_contract login
cargo test -p adss-center --test api_contract user_session
```

- [ ] **提交 SSO 和授权确认后端**

```text
git add center/src/routes.rs center/src/session.rs center/src/oidc/routes.rs center/tests/oidc_contract.rs
git commit -m "增加 SSO 授权确认流程"
```

## 授权码兑换与令牌

**文件：**

- 修改：`center/src/oidc/crypto.rs`
- 修改：`center/src/oidc/routes.rs`
- 修改：`center/tests/oidc_contract.rs`

- [ ] **写入 Web 和 Desktop Token 失败测试**

Web 测试使用 `Authorization: Basic base64(client_id:secret)` 和 PKCE；Desktop 测试只使用 `client_id` 和 PKCE。两者都验证返回：

```json
{
  "token_type": "Bearer",
  "expires_in": 300,
  "scope": "openid profile",
  "access_token": "<signed JWT>",
  "id_token": "<signed JWT>"
}
```

错误矩阵覆盖未知/过期/重复授权码、错误 verifier、client、secret、redirect URI、grant type、停用用户和停用客户端。并发提交相同 code 只能有一次成功。

- [ ] **运行测试并确认 Token 端点未实现**

```text
cargo test -p adss-center --test oidc_contract token
```

- [ ] **实现 Token 请求解析和客户端认证**

只接受 `application/x-www-form-urlencoded`。Web client 只接受 `client_secret_basic`，解析 Basic 时先解码 base64，再以第一个冒号分隔 ID/secret，并拒绝非 UTF-8、缺少分隔符或空凭据；Center 生成的 client ID 和 secret 只使用 URL-safe 无冒号字符，因此不接受额外转义形式。Desktop client 拒绝 secret。secret 计算 `sha256:` 摘要后常量时间比较。

验证 grant type 后原子消费授权码；再校验存储的 client、redirect URI 和 `pkce_s256(code_verifier)`。任何失败返回标准 `invalid_client`、`invalid_grant` 或 `unsupported_grant_type`，不泄露具体绑定失败项。

- [ ] **签发 ID Token 与 UserInfo access token**

ID Token audience 为 client ID，包含 code 中保存的 nonce/auth_time 和 scope 允许的身份字段。access token audience 为 issuer 下的 UserInfo 端点，包含 client ID 和 scope。两种 token 均固定 `RS256`、当前 `kid` 和 300 秒 TTL。

Token 成功和错误响应设置 `Cache-Control: no-store`、`Pragma: no-cache`，响应体不含 refresh token。

- [ ] **运行 Token、UserInfo 和全量 Center 测试**

```text
cargo test -p adss-center --test oidc_contract token
cargo test -p adss-center --test oidc_contract userinfo
cargo test -p adss-center
```

- [ ] **提交授权码兑换**

```text
git add center/src/oidc/crypto.rs center/src/oidc/routes.rs center/tests/oidc_contract.rs
git commit -m "增加 OIDC 授权码兑换"
```

## 管理端客户端页面

**文件：**

- 修改：`center/web/app/types/admin.ts`
- 修改：`center/web/app/composables/useAdminApi.ts`
- 修改：`center/web/app/components/admin/AdminShell.vue`
- 新增：`center/web/app/components/oauth/OAuthClientTable.vue`
- 新增：`center/web/app/components/oauth/OAuthClientEditor.vue`
- 新增：`center/web/app/pages/admin/clients.vue`
- 修改：`center/web/app/assets/css/main.css`

- [ ] **定义前端客户端契约和状态**

```ts
export type OAuthClientType = 'web' | 'desktop'

export interface OAuthClient {
  client_id: string
  name: string
  client_type: OAuthClientType
  redirect_uris: string[]
  allowed_scopes: string[]
  enabled: boolean
}
```

`useAdminApi` 增加 `oauthClients`、`loadOAuthClients`，并纳入 `resetData`、管理凭证初始化和 `refreshAll`。导航增加 Lucide `KeyRound` 图标和“登录客户端”，不手绘 SVG。

- [ ] **实现列表、编辑器和页面**

列表显示名称、client ID、类型、状态和 redirect URI 摘要；提供创建和编辑操作。编辑器使用原生文本输入、Web/Desktop 分段选择、启用复选框、scope 复选框和每行一个 redirect URI 的 textarea。

创建 Web client 或重新生成 secret 后，在同一个管理 Modal 中只显示一次 secret，提供 Lucide `Copy` 图标按钮和手工选择回退。Desktop 不显示 secret 操作。删除、停用、保存和重新生成期间锁定冲突操作，并复用域页面的 generation guard，避免关闭 Modal 后迟到响应重新显示 secret。

- [ ] **执行前端静态验证**

```text
cd center/web
bun run typecheck
bun run build
```

预期：类型检查和 Nuxt 静态生成成功；不运行或安装 Playwright，不启动浏览器。

- [ ] **提交客户端管理页面**

```text
git add center/web/app/types/admin.ts center/web/app/composables/useAdminApi.ts center/web/app/components/admin/AdminShell.vue center/web/app/components/oauth/OAuthClientTable.vue center/web/app/components/oauth/OAuthClientEditor.vue center/web/app/pages/admin/clients.vue center/web/app/assets/css/main.css
git commit -m "增加登录客户端管理页面"
```

## 登录继续与确认页面

**文件：**

- 新增：`center/web/app/types/oidc.ts`
- 新增：`center/web/app/pages/authorize.vue`
- 修改：`center/web/app/pages/login.vue`
- 修改：`center/web/app/pages/me.vue`
- 修改：`center/web/app/composables/useUserApi.ts`
- 修改：`center/web/app/assets/css/main.css`

- [ ] **实现受控登录继续地址**

登录页只在 query `continue` 解析为同 origin 且 pathname 严格等于 `/oauth2/authorize` 时使用它。存在合法 continue 时，不根据 localStorage Bearer token 自动跳到 `/me`；登录成功后跳回 continue。普通访问登录页保持现有 `/me` 行为。

`useUserApi.logout` 改为先 `POST /api/auth/logout`，无论响应如何都清除本地 Bearer token；`me.vue` 等待注销完成后再返回登录页。

- [ ] **实现逐次确认页**

`authorize.vue` 从当前 query 请求 `/api/oauth2/authorize/context`。401 时回到带内部 continue 的登录页；成功时显示客户端名称、当前用户和本次字段值。

确认与取消使用原生 `<form method="post" action="/oauth2/authorize">`，提交服务端返回的 CSRF 和经过上下文确认的授权参数，让浏览器直接跟随 303 到客户端。切换账号先调用 logout，再回到当前授权请求。页面不提供记住选择、自动同意或 `prompt=none` 路径。

- [ ] **校验响应式布局和文本边界**

CSS 使用现有颜色、间距、按钮和表单规范；确认页保持单层结构，不在卡片中嵌套卡片。客户端长名称、长用户名和字段值必须换行或截断显示，按钮在 320px 宽度下不溢出。

- [ ] **执行前端静态验证**

```text
cd center/web
bun run typecheck
bun run build
```

预期：类型检查和构建成功。此任务不启动浏览器、开发服务器或浏览器自动化。

- [ ] **提交登录与确认页面**

```text
git add center/web/app/types/oidc.ts center/web/app/pages/authorize.vue center/web/app/pages/login.vue center/web/app/pages/me.vue center/web/app/composables/useUserApi.ts center/web/app/assets/css/main.css
git commit -m "增加 OIDC 登录确认页面"
```

## 配置、部署与用户文档

**文件：**

- 修改：`center/.env.example`
- 修改：`deploy/center/center.env.example`
- 修改：`deploy/center/compose.yaml`
- 修改：`README.md`
- 修改：`docs/guide/deployment.md`
- 修改：`docs/guide/security.md`
- 修改：`docs/reference/api-contract.md`
- 修改：`docs/reference/data-model.md`
- 修改：`docs/reference/security-boundary.md`
- 修改：`scripts/test-docker-contract.ps1`

- [ ] **补充配置和 Docker 契约测试**

扩展 `scripts/test-docker-contract.ps1`，检查 Center 环境示例包含 `ADSS_OIDC_ISSUER`、`ADSS_OIDC_PRIVATE_KEY_FILE`，Compose 只读挂载 `oidc-private-key.pem` 到 `/run/secrets/oidc-private-key.pem`，发布内容不包含实际私钥。

- [ ] **运行脚本并确认因配置缺失而失败**

```powershell
pwsh -NoProfile -File scripts/test-docker-contract.ps1
```

- [ ] **更新运行配置和部署示例**

本地环境示例增加 issuer、私钥文件路径和默认关闭的 Web HTTP loopback 开关；部署 Compose 使用只读 secret 文件挂载，不把 PEM 内容放进 env、镜像或仓库。`center.env.example` 使用：

```text
ADSS_OIDC_ISSUER=https://center.example.com
ADSS_OIDC_PRIVATE_KEY_FILE=/run/secrets/oidc-private-key.pem
ADSS_OIDC_ALLOW_INSECURE_WEB_LOOPBACK_REDIRECTS=false
```

部署文档说明生成 RSA 私钥、限制文件权限、反向代理必须保持 issuer 对外 HTTPS 地址，以及更换私钥会立即替换 JWKS、旧 token 最多五分钟失效。

- [ ] **更新用户和参考文档**

README 主要功能加入“为 Web 和桌面系统提供 Center 账号统一登录”。API 契约记录公开端点、管理接口和错误边界；数据模型只增加两张 OAuth 表；安全文档记录 Cookie、RS256 私钥、禁止记录 token/授权请求、无 refresh token 和无远程会话撤销边界。文档不得使用阶段、当前实现或待办清单组织内容。

- [ ] **运行配置和文档检查**

```text
pwsh -NoProfile -File scripts/test-docker-contract.ps1
rg -n "第一阶段|当前已实现|待补" README.md docs/guide docs/reference center/.env.example deploy/center
git diff --check
```

预期：Docker 契约脚本通过；禁止词扫描无输出；diff 无空白错误。

- [ ] **提交配置和文档**

```text
git add center/.env.example deploy/center/center.env.example deploy/center/compose.yaml README.md docs/guide/deployment.md docs/guide/security.md docs/reference/api-contract.md docs/reference/data-model.md docs/reference/security-boundary.md scripts/test-docker-contract.ps1
git commit -m "补充 OIDC 部署和使用说明"
```

## 全量验证与收口

**文件：**

- 检查：全部本次改动文件

- [ ] **执行 Rust 格式化**

```text
cargo fmt --all
cargo fmt --manifest-path crates/store/Cargo.toml
cargo fmt --manifest-path crates/protocol/Cargo.toml
```

- [ ] **执行全部 Rust 测试**

```text
cargo test --workspace
cargo test --manifest-path crates/store/Cargo.toml
cargo test --manifest-path crates/protocol/Cargo.toml
```

预期：workspace、Store 和 Protocol 测试全部通过。

- [ ] **执行全部 Rust Clippy**

```text
cargo clippy --all-targets --all-features -- -D warnings
cargo clippy --manifest-path crates/store/Cargo.toml --all-targets --all-features -- -D warnings
cargo clippy --manifest-path crates/protocol/Cargo.toml --all-targets --all-features -- -D warnings
```

预期：全部命令零警告通过；不使用 `CARGO_TARGET_DIR` 绕开锁或权限问题。

- [ ] **执行 Nuxt 和交付检查**

```text
cd center/web
bun run typecheck
bun run build
cd ../../
pwsh -NoProfile -File scripts/test-docker-contract.ps1
git diff --check
git status --short
```

不运行 Playwright，不启动浏览器或开发服务器。真实 Web 客户端、桌面 loopback 客户端和 OpenID Foundation conformance suite 保留为打包后的外部验收，未执行时在交付说明中明确记录。

- [ ] **审查提交和工作树**

```text
git log --oneline --decorate -n 12
git status --short --branch
```

预期：每个提交只包含对应逻辑单元，工作树干净；如格式化产生未提交差异，按所属逻辑单元检查并提交，不能把不相关变更混入收口提交。
