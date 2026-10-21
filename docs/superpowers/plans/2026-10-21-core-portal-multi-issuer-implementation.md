# Core、Portal 多 issuer 实现计划

> 2026-10-22 执行提示：服务目录和 crate/二进制已重命名为 `core`、`portal`、`syncer`。下文保留计划编写时的 `center`、`gateway`、`connector` 历史路径，阅读时按同名映射理解，实际命令以当前仓库目录为准。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让三个 Portal 分别作为 OIDC issuer，由 Core 按请求 Host 统一签发和校验，同时保持客户端记录全局、Syncer 直连 Core。

**Architecture:** Core 保存 `OIDC_ISSUERS` 白名单和唯一 RSA 密钥，OIDC 端点从当前 Host 解析 issuer；Portal 用 `PUBLIC_ISSUER` 覆盖转发 Host，并支持前置代理终止 TLS 的 HTTP 监听模式。授权码记录绑定 issuer，Token 和 UserInfo 都只接受当前 issuer。

**Tech Stack:** Rust 2024、Axum、SeaORM、SQLite、reqwest、axum-server、Bun/Nuxt（本次不修改前端源码）。

**Spec:** `docs/superpowers/specs/2026-10-21-core-portal-multi-issuer-design.md`

## Global Constraints

- Core 地址固定为 `https://core.example.com`，但 Core 不再作为 OIDC issuer。
- 三个 Portal issuer 固定为：
  - 内网入口 A：`https://auth-a.example.com`
  - 内网入口 B：`https://auth-b.example.com`
  - 外网：`https://auth-c.example.com`
- Core 的目标配置是 `OIDC_ISSUERS=https://auth-a.example.com,https://auth-b.example.com,https://auth-c.example.com`。
- Host 统一转小写、去掉域名末尾点、去掉默认 `:443`；非默认端口保留。
- `core.example.com` 不在 issuer 白名单；直接访问 discovery、JWKS、authorize、token、userinfo 必须返回 `404`。
- 三个 issuer 共用 Core 的一把 RSA 私钥和同一份 JWKS。
- 授权码和 access token 必须绑定 issuer，跨 Portal 使用分别返回 `invalid_grant` 和 `401 invalid_token`。
- 三个 Portal 独立 Cookie，不实现跨 Portal SSO。
- OIDC 客户端记录保持全局，不增加 issuer 字段，不绑定客户端与 issuer。
- 不重命名 crate、二进制、镜像、Docker 服务或 MSI 服务名；术语只改文档。
- `gateway/web` 本次预计不需要修改；`gateway/static` 不作为本地开发输入，也不因本计划重新生成。
- 区域 A、内网入口 B Portal 设置 `ADMIN_ENABLED=true`；外网 Portal 设置 `ADMIN_ENABLED=false`。
- 本机没有 Docker，E 盘空间紧张；不执行 release 镜像构建，不把镜像构建或线上部署写成已验证。
- 已发布 `0.2.0` 镜像的 UID `10001` 与当前源码 `1000` 的差异不在本计划修复范围。
- 提交、推送、打 tag、发布分别授权；本计划中的提交步骤只有在用户明确授权后执行。

## File Structure

| 区域 | 文件 | 职责 |
| --- | --- | --- |
| Store | `crates/store/src/models.rs` | `AuthorizationCodeRecord` 增加 `issuer` |
| Store | `crates/store/src/entities.rs` | SeaORM 实体增加 `issuer` |
| Store | `crates/store/src/oauth.rs` | ActiveModel 与实体映射增加 `issuer` |
| Store | `crates/store/src/repository.rs` | 建表、旧库补列、授权码读写 |
| Core | `center/src/oidc/config.rs` | issuer 列表解析、Host 归一化、旧变量兼容 |
| Core | `center/src/oidc/mod.rs` | 按显式 issuer 签发和校验 Token |
| Core | `center/src/oidc/routes.rs` | Host 选择 issuer、404、授权码绑定、Token/UserInfo |
| Core | `center/src/state.rs` | 测试和多 issuer 状态构造 |
| Core tests | `center/tests/oidc_contract.rs` | 多 issuer 契约测试和现有测试迁移 |
| Core tests | `center/tests/api_contract.rs` | `OIDC_ISSUERS` 环境变量和启动校验 |
| Portal | `gateway/src/lib.rs` | `PUBLIC_ISSUER`、Host 覆盖、监听模式配置 |
| Portal | `gateway/src/main.rs` | HTTP/HTTPS 分支 |
| Portal tests | `gateway/src/lib.rs` 内测试 | issuer 校验、Host 覆盖、TLS 配置组合 |
| Docs | `README.md`、`gateway/README.md`、`deploy/center/.env.example`、`docs/**` | 配置、拓扑、数据模型、迁移顺序和架构图 |

## Core 请求链路

```text
浏览器
  -> Portal 域名
  -> Portal 用 PUBLIC_ISSUER 覆盖 Host
  -> Core 按 Host 查 OIDC_ISSUERS
  -> 当前 issuer 进入 discovery、authorize、token、userinfo
  -> 授权码和 Token 绑定当前 issuer
```

Syncer 仍直接访问 `https://core.example.com` 的 `/api/connector/*`，不经过 issuer 解析，也不受 OIDC Host 白名单影响。

## 授权码 issuer 持久化与旧库迁移

**Files:**
- Modify: `crates/store/src/models.rs`
- Modify: `crates/store/src/entities.rs`
- Modify: `crates/store/src/oauth.rs`
- Modify: `crates/store/src/repository.rs`
- Test: `crates/store/tests/oauth_repository_contract.rs`
- Modify: `center/tests/oidc_contract.rs`

**Interfaces:**
- Consumes: 现有 `Repository::store_authorization_code`、`consume_authorization_code`、`consume_authorization_code_for_exchange`。
- Produces: `AuthorizationCodeRecord.issuer: String`，以及旧表自动补 `issuer TEXT NOT NULL DEFAULT ''`。

- [ ] **Step 1: 先写 issuer 往返和旧表迁移的失败测试**

在 `crates/store/tests/oauth_repository_contract.rs` 的测试模块中加入：

```rust
const ISSUER: &str = "https://auth-a.example.com";

#[tokio::test]
async fn authorization_code_issuer_round_trips() {
    let repository = sqlite_repository().await;
    let code = authorization_code("issuer-code", 2_000);

    repository
        .store_authorization_code(code.clone())
        .await
        .unwrap();

    assert_eq!(
        repository
            .consume_authorization_code("issuer-code", 1_999)
            .await
            .unwrap(),
        Some(code)
    );
}

#[tokio::test]
async fn old_authorization_code_table_gains_empty_issuer_column() {
    let (database_url, database_path) = sqlite_file_database_url("issuer-migration");
    let db = Database::connect(&database_url).await.unwrap();
    db.execute_unprepared(
        r#"
CREATE TABLE oauth_authorization_codes (
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
"#,
    )
    .await
    .unwrap();
    db.execute_unprepared(
        r#"
INSERT INTO oauth_authorization_codes (
    code_hash, client_id, employee_id, redirect_uri, scopes,
    nonce, code_challenge, auth_time, expires_at
) VALUES ('old-code', 'client', '1001', 'https://client.example.com/callback',
          '["openid"]', 'nonce', 'challenge', 1000, 2000)
"#,
    )
    .await
    .unwrap();
    db.close().await.unwrap();

    let repository = Repository::connect(&database_url).await.unwrap();
    repository.initialize_schema().await.unwrap();

    let old_code = repository
        .consume_authorization_code("old-code", 1_999)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(old_code.issuer, "");

    std::fs::remove_file(database_path).ok();
}
```

把测试 helper `authorization_code` 改为：

```rust
fn authorization_code(code_hash: &str, expires_at: i64) -> AuthorizationCodeRecord {
    AuthorizationCodeRecord {
        code_hash: code_hash.to_string(),
        issuer: ISSUER.to_string(),
        client_id: "client".to_string(),
        employee_id: "1001".to_string(),
        redirect_uri: "https://client.example.com/callback".to_string(),
        scopes: vec!["openid".to_string(), "profile".to_string()],
        nonce: "nonce-1".to_string(),
        code_challenge: "challenge-1".to_string(),
        auth_time: 1_000,
        expires_at,
    }
}
```

- [ ] **Step 2: 运行 store 测试确认失败**

在 `crates/store` 下执行：

```text
cargo test authorization_code_issuer_round_trips -- --nocapture
cargo test old_authorization_code_table_gains_empty_issuer_column -- --nocapture
```

Expected: 编译失败，提示 `AuthorizationCodeRecord` 缺少 `issuer`，或数据库查询缺少该列。

- [ ] **Step 3: 增加模型、实体和映射字段**

`crates/store/src/models.rs`：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorizationCodeRecord {
    pub code_hash: String,
    pub issuer: String,
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

`crates/store/src/entities.rs` 的 `oauth_authorization_code::Model` 增加：

```rust
pub issuer: String,
```

`crates/store/src/oauth.rs` 的 `authorization_code_active_model` 增加：

```rust
issuer: Set(record.issuer),
```

`TryFrom<oauth_authorization_code::Model> for AuthorizationCodeRecord` 增加：

```rust
issuer: model.issuer,
```

- [ ] **Step 4: 新建表和旧库补列**

`crates/store/src/repository.rs` 的 `CREATE TABLE oauth_authorization_codes` 增加：

```sql
issuer TEXT NOT NULL DEFAULT '',
```

在 `initialize_schema` 中，建表语句之后、`ensure_metadata` 之前调用：

```rust
self.ensure_authorization_code_issuer_column().await?;
```

新增 helper，沿用现有 `ensure_user_name_columns` 的 SQLite 补列方式：

```rust
async fn ensure_authorization_code_issuer_column(&self) -> anyhow::Result<()> {
    let columns = self
        .db
        .query_all_raw(Statement::from_string(
            self.db.get_database_backend(),
            "PRAGMA table_info(oauth_authorization_codes)",
        ))
        .await?
        .into_iter()
        .map(|row| row.try_get::<String>("", "name"))
        .collect::<Result<Vec<_>, _>>()?;

    if !columns.iter().any(|column| column == "issuer") {
        self.db
            .execute_unprepared(
                "ALTER TABLE oauth_authorization_codes ADD COLUMN issuer TEXT NOT NULL DEFAULT ''",
            )
            .await?;
    }
    Ok(())
}
```

- [ ] **Step 5: 更新 Center 测试中的授权码构造**

在 `center/tests/oidc_contract.rs` 的 `seed_token_code` 中增加：

```rust
issuer: TEST_OIDC_ISSUER.to_string(),
```

同时修正所有 `AuthorizationCodeRecord` 结构体字面量，保证 root workspace 仍能编译。

- [ ] **Step 6: 运行 store 和 Center 编译检查**

在仓库根目录执行：

```text
cargo test -p adscope-center --test oidc_contract --no-run
```

在 `crates/store` 下执行：

```text
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

Expected: store 测试通过；Center 测试可编译，授权码 issuer 行为尚未接入路由。

- [ ] **Step 7: 提交存储层变更（仅在用户授权提交后）**

```text
git add crates/store/src/models.rs crates/store/src/entities.rs crates/store/src/oauth.rs crates/store/src/repository.rs crates/store/tests/oauth_repository_contract.rs center/tests/oidc_contract.rs
git commit -m "为授权码绑定 issuer"
```

## Core issuer 配置与按 Host 隔离

**Files:**
- Modify: `center/src/oidc/config.rs`
- Modify: `center/src/oidc/mod.rs`
- Modify: `center/src/oidc/routes.rs`
- Modify: `center/src/state.rs`
- Modify: `center/tests/api_contract.rs`
- Modify: `center/tests/oidc_contract.rs`

**Interfaces:**
- Consumes: Store 已提供 `AuthorizationCodeRecord.issuer`。
- Produces:
  - `OidcConfig::from_issuers(&[&str], Vec<u8>, bool) -> anyhow::Result<Self>`（严格 Host 匹配）
  - `OidcConfig::new_single_issuer_legacy(&str, Vec<u8>, bool) -> anyhow::Result<Self>`（过渡兼容）
  - `OidcConfig::issuer_for_host(&self, Option<&str>) -> Option<&str>`
  - `AppState::new_for_tests_with_issuers(Repository, impl Into<String>, &[&str], &[u8]) -> Self`
  - `OidcService::issue_id_token_for_issuer(&self, &str, &str, &str, u64, &str) -> anyhow::Result<String>`
  - `OidcService::issue_access_token_for_issuer(&self, &str, &str, &str, &str) -> anyhow::Result<String>`
  - `OidcService::verify_access_token_for_issuer(&self, &str, &str) -> anyhow::Result<AccessTokenClaims>`

- [ ] **Step 1: 写 issuer 解析、归一化和重复检测的失败测试**

在 `center/src/oidc/config.rs` 的测试模块加入：

```rust
#[test]
fn multiple_issuers_are_normalized_and_selected_by_host() {
    let config = OidcConfig::from_issuers(
        &[
            "https://auth-a.example.com.:443",
            "https://auth-b.example.com",
            "https://auth-c.example.com:8443",
        ],
        PRIVATE_KEY.to_vec(),
        false,
    )
    .unwrap();

    assert_eq!(
        config.issuer_for_host(Some("auth-a.example.com.:443")),
        Some("https://auth-a.example.com")
    );
    assert_eq!(
        config.issuer_for_host(Some("auth-b.example.com")),
        Some("https://auth-b.example.com")
    );
    assert_eq!(
        config.issuer_for_host(Some("auth-c.example.com:8443")),
        Some("https://auth-c.example.com:8443")
    );
    assert_eq!(config.issuer_for_host(Some("core.example.com")), None);
    assert_eq!(config.issuer_for_host(None), None);
}

#[test]
fn issuer_lists_reject_invalid_and_duplicate_origins() {
    for issuer in [
        "http://auth-a.example.com",
        "https://auth-a.example.com/path",
        "https://auth-a.example.com?query=value",
        "https://auth-a.example.com#fragment",
        "https://user@auth-a.example.com",
    ] {
        assert!(
            OidcConfig::from_issuers(&[issuer], PRIVATE_KEY.to_vec(), false).is_err(),
            "unexpectedly accepted {issuer}"
        );
    }

    assert!(
        OidcConfig::from_issuers(
            &["https://auth-a.example.com", "https://auth-a.example.com.:443"],
            PRIVATE_KEY.to_vec(),
            false,
        )
        .is_err()
    );
    assert!(
        OidcConfig::from_issuers(&[], PRIVATE_KEY.to_vec(), false).is_err()
    );
}

#[test]
fn legacy_single_issuer_ignores_request_host() {
    let config = OidcConfig::new_single_issuer_legacy(
        "https://center.example.test",
        PRIVATE_KEY.to_vec(),
        false,
    )
    .unwrap();

    assert_eq!(
        config.issuer_for_host(Some("attacker.example.test")),
        Some("https://center.example.test")
    );
    assert_eq!(
        config.issuer_for_host(None),
        Some("https://center.example.test")
    );
}
```

- [ ] **Step 2: 运行配置测试确认失败**

在仓库根目录执行：

```text
cargo test -p adscope-center oidc::config::tests -- --nocapture
```

Expected: 编译失败，提示 `from_issuers`、`issuer_for_host` 或 `new_single_issuer_legacy` 不存在。

- [ ] **Step 3: 实现 issuer 列表和 Host 归一化**

`center/src/oidc/config.rs` 的常量改为：

```rust
const ISSUERS_ENV: &str = "OIDC_ISSUERS";
const LEGACY_ISSUER_ENV: &str = "OIDC_ISSUER";
```

新增内部类型：

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
struct OidcIssuer {
    issuer: String,
    host: String,
}

impl OidcIssuer {
    fn parse(value: &str) -> anyhow::Result<Self> {
        let value = value.trim();
        let url = Url::parse(value).context("OIDC issuer must be an absolute URL")?;
        ensure!(url.scheme() == "https", "OIDC issuer must use HTTPS");
        ensure!(url.host().is_some(), "OIDC issuer must include a host");
        ensure!(
            url.username().is_empty() && url.password().is_none(),
            "OIDC issuer must not include userinfo"
        );
        ensure!(
            url.query().is_none(),
            "OIDC issuer must not include a query"
        );
        ensure!(
            url.fragment().is_none(),
            "OIDC issuer must not include a fragment"
        );
        ensure!(
            url.path() == "/",
            "OIDC issuer path must be '/' or empty"
        );

        let host = url
            .host_str()
            .expect("validated issuer has a host")
            .trim_end_matches('.')
            .to_ascii_lowercase();
        ensure!(!host.is_empty(), "OIDC issuer host must not be empty");

        let mut normalized = url;
        normalized
            .set_host(Some(&host))
            .expect("validated issuer host must remain valid");
        normalized.set_path("");
        let issuer = normalized.as_str().trim_end_matches('/').to_string();
        let host = issuer
            .strip_prefix("https://")
            .expect("validated issuer uses HTTPS")
            .to_string();

        Ok(Self { issuer, host })
    }

    fn as_str(&self) -> &str {
        &self.issuer
    }
}

fn normalize_host(value: &str) -> Option<String> {
    let url = Url::parse(&format!("https://{value}")).ok()?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let host = url.host_str()?.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    let mut normalized = url;
    normalized.set_host(Some(&host)).ok()?;
    normalized.set_path("");
    Some(
        normalized
            .as_str()
            .trim_start_matches("https://")
            .trim_end_matches('/')
            .to_string(),
    )
}
```

`OidcConfig` 改为保存 `issuers: Vec<OidcIssuer>` 和 `legacy_single_issuer: bool`。构造函数：

```rust
pub fn new_single_issuer_legacy(
    issuer: &str,
    private_key_pem: Vec<u8>,
    allow_insecure_web_loopback_redirects: bool,
) -> anyhow::Result<Self> {
    Self::build(
        vec![OidcIssuer::parse(issuer)?],
        private_key_pem,
        allow_insecure_web_loopback_redirects,
        true,
    )
}

pub fn from_issuers(
    issuers: &[&str],
    private_key_pem: Vec<u8>,
    allow_insecure_web_loopback_redirects: bool,
) -> anyhow::Result<Self> {
    ensure!(!issuers.is_empty(), "at least one OIDC issuer is required");
    let issuers = issuers
        .iter()
        .map(|issuer| OidcIssuer::parse(issuer))
        .collect::<anyhow::Result<Vec<_>>>()?;
    ensure_unique_hosts(&issuers)?;
    Self::build(
        issuers,
        private_key_pem,
        allow_insecure_web_loopback_redirects,
        false,
    )
}

fn ensure_unique_hosts(issuers: &[OidcIssuer]) -> anyhow::Result<()> {
    let mut hosts = std::collections::HashSet::new();
    for issuer in issuers {
        ensure!(
            hosts.insert(issuer.host.as_str()),
            "OIDC_ISSUERS must not contain duplicate hosts"
        );
    }
    Ok(())
}

fn parse_issuer_list(value: &str) -> anyhow::Result<Vec<OidcIssuer>> {
    let values = value.split(',').map(str::trim).collect::<Vec<_>>();
    ensure!(
        !values.is_empty() && values.iter().all(|value| !value.is_empty()),
        "OIDC_ISSUERS must contain at least one issuer"
    );
    let issuers = values
        .iter()
        .map(|value| OidcIssuer::parse(value))
        .collect::<anyhow::Result<Vec<_>>>()?;
    ensure_unique_hosts(&issuers)?;
    Ok(issuers)
}

fn build(
    issuers: Vec<OidcIssuer>,
    private_key_pem: Vec<u8>,
    allow_insecure_web_loopback_redirects: bool,
    legacy_single_issuer: bool,
) -> anyhow::Result<Self> {
    ensure!(!issuers.is_empty(), "at least one OIDC issuer is required");
    Ok(Self {
        issuers,
        private_key_pem,
        allow_insecure_web_loopback_redirects,
        legacy_single_issuer,
        authorization_code_ttl: Duration::from_secs(AUTHORIZATION_CODE_TTL_SECONDS),
        token_ttl: Duration::from_secs(TOKEN_TTL_SECONDS),
    })
}
```

`from_env_with_private_key_path` 的 issuer 选择必须是：

```rust
let issuers_env = optional_env(ISSUERS_ENV)?;
let legacy_env = optional_env(LEGACY_ISSUER_ENV)?;
let (issuers, legacy_single_issuer) = match (issuers_env, legacy_env) {
    (Some(_), Some(_)) => bail!(
        "{ISSUERS_ENV} and {LEGACY_ISSUER_ENV} must not both be set"
    ),
    (Some(value), None) => (parse_issuer_list(&value)?, false),
    (None, Some(value)) => (vec![OidcIssuer::parse(&value)?], true),
    (None, None) => bail!("{ISSUERS_ENV} or {LEGACY_ISSUER_ENV} is required"),
};
```

`optional_env` 按以下语义实现，保证变量存在但非 UTF-8 时不会被误认为未设置：

```rust
fn optional_env(name: &str) -> anyhow::Result<Option<String>> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(error.into()),
    }
}
```

`OIDC_ISSUERS` 和 `OIDC_ISSUER` 同时存在时，必须在读取私钥之前失败。

新增：

```rust
pub fn issuer_for_host(&self, host: Option<&str>) -> Option<&str> {
    if self.legacy_single_issuer {
        return self.issuers.first().map(OidcIssuer::as_str);
    }
    let host = normalize_host(host?)?;
    self.issuers
        .iter()
        .find(|issuer| issuer.host == host)
        .map(OidcIssuer::as_str)
}

pub fn is_legacy_single_issuer(&self) -> bool {
    self.legacy_single_issuer
}
```

- [ ] **Step 4: 把 Token 签发和校验改为显式 issuer**

先在 `center/src/oidc/mod.rs` 的测试模块加入：

```rust
#[test]
fn tokens_use_explicit_issuer() {
    let service = test_service();
    for issuer in [
        "https://auth-a.example.com",
        "https://auth-b.example.com",
        "https://auth-c.example.com",
    ] {
        let access = service
            .issue_access_token_for_issuer(issuer, "1001", "client-web", "openid")
            .unwrap();
        let access_claims = service
            .verify_access_token_for_issuer(issuer, &access)
            .unwrap();
        assert_eq!(access_claims.iss, issuer);
        assert_eq!(
            access_claims.aud,
            format!("{issuer}/oauth2/userinfo")
        );

        let id = service
            .issue_id_token_for_issuer(
                issuer,
                "1001",
                "client-web",
                1_700_000_000,
                "nonce",
            )
            .unwrap();
        let id_claims = service
            .verify_id_token_for_issuer(issuer, &id, "client-web")
            .unwrap();
        assert_eq!(id_claims.iss, issuer);
        assert_eq!(id_claims.aud, "client-web");
    }
}
```

Expected: 编译失败，提示方法尚未使用显式 issuer。

再更新实现：

在 `center/src/oidc/mod.rs` 中删除对所有“当前全局 issuer”的读取，改为：

```rust
pub fn issue_id_token_for_issuer(
    &self,
    issuer: &str,
    subject: &str,
    audience: &str,
    auth_time: u64,
    nonce: &str,
) -> anyhow::Result<String>

pub(crate) fn issue_id_token_with_user_claims_for_issuer(
    &self,
    issuer: &str,
    subject: &str,
    audience: &str,
    auth_time: u64,
    nonce: &str,
    user_claims: IdTokenUserClaims,
) -> anyhow::Result<String>

pub fn verify_id_token_for_issuer(
    &self,
    issuer: &str,
    token: &str,
    audience: &str,
) -> anyhow::Result<IdTokenClaims>

pub fn issue_access_token_for_issuer(
    &self,
    issuer: &str,
    subject: &str,
    client_id: &str,
    scope: &str,
) -> anyhow::Result<String>

pub fn issue_access_token_at_for_issuer(
    &self,
    issuer: &str,
    subject: &str,
    client_id: &str,
    scope: &str,
    issued_at: u64,
) -> anyhow::Result<String>

pub fn verify_access_token_for_issuer(
    &self,
    issuer: &str,
    token: &str,
) -> anyhow::Result<AccessTokenClaims>
```

`IdTokenClaims.iss` 和 `AccessTokenClaims.iss` 都赋值为传入的 `issuer`；`userinfo_audience` 改为：

```rust
fn userinfo_audience(issuer: &str) -> String {
    format!("{issuer}/oauth2/userinfo")
}
```

`verify_*_for_issuer` 必须设置：

```rust
validation.set_issuer(&[issuer]);
validation.set_audience(&[audience.as_str()]);
```

access token 的 audience 使用 `userinfo_audience(issuer)`。

- [ ] **Step 5: 写多 issuer 路由和跨 issuer 拒绝的失败测试**

先把现有 `seed_token_code` 改成带 issuer 的实现：

```rust
async fn seed_token_code_for_issuer(
    repository: &Repository,
    code: &str,
    client_id: &str,
    redirect_uri: &str,
    expires_at: i64,
    issuer: &str,
) {
    repository
        .store_authorization_code(adscope_store::AuthorizationCodeRecord {
            code_hash: sha256_token(code),
            issuer: issuer.to_string(),
            client_id: client_id.to_string(),
            employee_id: "1001".to_string(),
            redirect_uri: redirect_uri.to_string(),
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
                "phone".to_string(),
            ],
            nonce: OIDC_NONCE.to_string(),
            code_challenge: TOKEN_CODE_CHALLENGE.to_string(),
            auth_time: 1_700_000_000,
            expires_at,
        })
        .await
        .unwrap();
}

async fn seed_token_code(
    repository: &Repository,
    code: &str,
    client_id: &str,
    redirect_uri: &str,
    expires_at: i64,
) {
    seed_token_code_for_issuer(
        repository,
        code,
        client_id,
        redirect_uri,
        expires_at,
        TEST_OIDC_ISSUER,
    )
    .await;
}
```

在 `center/tests/oidc_contract.rs` 增加常量和测试：

```rust
const PORTAL_A_ISSUER: &str = "https://auth-a.example.com";
const PORTAL_A_HOST: &str = "auth-a.example.com";
const PORTAL_B_ISSUER: &str = "https://auth-b.example.com";
const PORTAL_B_HOST: &str = "auth-b.example.com";
const PORTAL_C_ISSUER: &str = "https://auth-c.example.com";
const PORTAL_C_HOST: &str = "auth-c.example.com";

fn with_host(mut request: Request<Body>, host: &str) -> Request<Body> {
    request.headers_mut().insert(
        header::HOST,
        host.parse().expect("test host must be a valid header value"),
    );
    request
}

async fn multi_issuer_test_app() -> TestApp {
    let repository = Repository::connect("sqlite::memory:").await.unwrap();
    repository.initialize_schema().await.unwrap();
    let state = AppState::new_for_tests_with_issuers(
        repository.clone(),
        TEST_ENCRYPTION_KEY,
        &[PORTAL_A_ISSUER, PORTAL_B_ISSUER, PORTAL_C_ISSUER],
        TEST_OIDC_PRIVATE_KEY,
    );
    let oidc = state.oidc.clone();
    TestApp {
        app: build_router(state),
        repository,
        oidc,
    }
}

#[tokio::test]
async fn multi_issuer_discovery_uses_normalized_request_host() {
    let TestApp { app, .. } = multi_issuer_test_app().await;

    for (host, issuer) in [
        (PORTAL_A_HOST, PORTAL_A_ISSUER),
        (PORTAL_B_HOST, PORTAL_B_ISSUER),
        (PORTAL_C_HOST, PORTAL_C_ISSUER),
    ] {
        let response = app
            .clone()
            .oneshot(with_host(
                empty_request(Method::GET, "/.well-known/openid-configuration"),
                host,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["issuer"], issuer);
        assert_eq!(
            body["authorization_endpoint"],
            format!("{issuer}/oauth2/authorize")
        );
        assert_eq!(body["token_endpoint"], format!("{issuer}/oauth2/token"));
        assert_eq!(
            body["userinfo_endpoint"],
            format!("{issuer}/oauth2/userinfo")
        );
        assert_eq!(body["jwks_uri"], format!("{issuer}/oauth2/jwks"));
    }

    let normalized = app
        .oneshot(with_host(
            empty_request(Method::GET, "/.well-known/openid-configuration"),
            "auth-a.example.com.:443",
        ))
        .await
        .unwrap();

    assert_eq!(normalized.status(), StatusCode::OK);
    let body = json_body(normalized).await;
    assert_eq!(body["issuer"], PORTAL_A_ISSUER);
}

#[tokio::test]
async fn unknown_and_missing_hosts_return_404_for_oidc_routes() {
    let TestApp { app, .. } = multi_issuer_test_app().await;
    for (method, path) in [
        (Method::GET, "/.well-known/openid-configuration"),
        (Method::GET, "/oauth2/jwks"),
        (Method::GET, "/oauth2/authorize"),
        (Method::POST, "/oauth2/token"),
        (Method::GET, "/oauth2/userinfo"),
    ] {
        let request = if method == Method::POST {
            with_host(
                raw_token_request(
                    Body::empty(),
                    "application/x-www-form-urlencoded",
                    None,
                ),
                "core.example.com",
            )
        } else {
            with_host(empty_request(method.clone(), path), "core.example.com")
        };
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{} {path}",
            method.as_str()
        );
    }

    let missing_host = app
        .oneshot(empty_request(
            Method::GET,
            "/.well-known/openid-configuration",
        ))
        .await
        .unwrap();
    assert_eq!(missing_host.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn authorization_code_issuer_must_match_token_request_issuer() {
    let TestApp {
        app,
        repository,
        oidc,
    } = multi_issuer_test_app().await;
    seed_user(&repository).await;
    seed_token_client(&repository, OAuthClientType::Desktop).await;
    seed_token_code_for_issuer(
        &repository,
        "a-code",
        TOKEN_DESKTOP_CLIENT_ID,
        TOKEN_DESKTOP_REDIRECT_URI,
        unix_seconds() as i64 + 120,
        PORTAL_A_ISSUER,
    )
    .await;

    let wrong_issuer = app
        .clone()
        .oneshot(with_host(
            token_request(
                TOKEN_DESKTOP_CLIENT_ID,
                TOKEN_DESKTOP_REDIRECT_URI,
                "a-code",
                None,
            ),
            PORTAL_B_HOST,
        ))
        .await
        .unwrap();
    assert_token_error(
        wrong_issuer,
        StatusCode::BAD_REQUEST,
        "invalid_grant",
        None,
    )
    .await;

    seed_token_code_for_issuer(
        &repository,
        "legacy-code-without-issuer",
        TOKEN_DESKTOP_CLIENT_ID,
        TOKEN_DESKTOP_REDIRECT_URI,
        unix_seconds() as i64 + 120,
        "",
    )
    .await;
    let legacy_code = app
        .clone()
        .oneshot(with_host(
            token_request(
                TOKEN_DESKTOP_CLIENT_ID,
                TOKEN_DESKTOP_REDIRECT_URI,
                "legacy-code-without-issuer",
                None,
            ),
            PORTAL_A_HOST,
        ))
        .await
        .unwrap();
    assert_token_error(
        legacy_code,
        StatusCode::BAD_REQUEST,
        "invalid_grant",
        None,
    )
    .await;

    seed_token_code_for_issuer(
        &repository,
        "a-code-2",
        TOKEN_DESKTOP_CLIENT_ID,
        TOKEN_DESKTOP_REDIRECT_URI,
        unix_seconds() as i64 + 120,
        PORTAL_A_ISSUER,
    )
    .await;
    let matched = app
        .oneshot(with_host(
            token_request(
                TOKEN_DESKTOP_CLIENT_ID,
                TOKEN_DESKTOP_REDIRECT_URI,
                "a-code-2",
                None,
            ),
            PORTAL_A_HOST,
        ))
        .await
        .unwrap();
    assert_token_headers(&matched, StatusCode::OK);
    let body = json_body(matched).await;
    let access_claims = oidc
        .verify_access_token_for_issuer(
            PORTAL_A_ISSUER,
            body["access_token"].as_str().unwrap(),
        )
        .unwrap();
    assert_eq!(access_claims.iss, PORTAL_A_ISSUER);
    assert_eq!(
        access_claims.aud,
        "https://auth-a.example.com/oauth2/userinfo"
    );
    let id_claims = oidc
        .verify_id_token_for_issuer(
            PORTAL_A_ISSUER,
            body["id_token"].as_str().unwrap(),
            TOKEN_DESKTOP_CLIENT_ID,
        )
        .unwrap();
    assert_eq!(id_claims.iss, PORTAL_A_ISSUER);
    assert_eq!(id_claims.aud, TOKEN_DESKTOP_CLIENT_ID);
}

#[tokio::test]
async fn access_token_is_rejected_by_another_issuer_userinfo() {
    let TestApp {
        app,
        repository,
        oidc,
    } = multi_issuer_test_app().await;
    seed_user(&repository).await;
    seed_oidc_client(&repository).await;
    let token = oidc
        .issue_access_token_for_issuer(PORTAL_A_ISSUER, "1001", OIDC_CLIENT_ID, "openid")
        .unwrap();

    let wrong_issuer = app
        .clone()
        .oneshot(with_host(
            auth_request(Method::GET, "/oauth2/userinfo", &token),
            PORTAL_B_HOST,
        ))
        .await
        .unwrap();
    assert_invalid_token(wrong_issuer).await;

    let right_issuer = app
        .oneshot(with_host(
            auth_request(Method::GET, "/oauth2/userinfo", &token),
            PORTAL_A_HOST,
        ))
        .await
        .unwrap();
    assert_eq!(right_issuer.status(), StatusCode::OK);
}
```

同时把现有授权确认测试 `authorization_approve_stores_hash_bound_record_and_allows_new_confirmations` 增加 issuer 断言：

```rust
assert_eq!(stored.issuer, TEST_OIDC_ISSUER);
```

再新增三个 host 参数化的测试 helper：

```rust
async fn login_cookie_for_host(app: &axum::Router, host: &str) -> String {
    let response = app
        .clone()
        .oneshot(with_host(
            json_request(
                Method::POST,
                "/api/auth/login",
                &UserLoginRequest {
                    username: "test-user".to_string(),
                    password: "UserPass123!".to_string(),
                },
            ),
            host,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    session_cookie_value(response.headers()[header::SET_COOKIE].to_str().unwrap())
}

async fn authorization_csrf_for_host(
    app: &axum::Router,
    cookie: &str,
    host: &str,
) -> String {
    let context_uri = format!(
        "/api/oauth2/authorize/context?{}",
        authorization_uri().split_once('?').unwrap().1
    );
    let response = app
        .clone()
        .oneshot(with_host(
            cookie_request(Method::GET, &context_uri, cookie),
            host,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await["csrf_token"]
        .as_str()
        .unwrap()
        .to_string()
}

fn form_request_for_host(
    uri: &str,
    form: &str,
    cookie: &str,
    host: &str,
) -> Request<Body> {
    with_host(form_request(uri, form, cookie), host)
}

#[tokio::test]
async fn multi_issuer_authorization_approval_records_current_issuer() {
    let TestApp {
        app,
        repository,
        ..
    } = multi_issuer_test_app().await;
    seed_user(&repository).await;
    seed_oidc_client(&repository).await;
    let cookie = login_cookie_for_host(&app, PORTAL_A_HOST).await;
    let csrf = authorization_csrf_for_host(&app, &cookie, PORTAL_A_HOST).await;
    let response = app
        .oneshot(form_request_for_host(
            "/oauth2/authorize",
            &authorization_form("approve", &csrf),
            &cookie,
            PORTAL_A_HOST,
        ))
        .await
        .unwrap();
    let code = approved_code(response);
    let stored = repository
        .consume_authorization_code(&sha256_token(&code), unix_seconds() as i64)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(stored.issuer, PORTAL_A_ISSUER);
}
```

- [ ] **Step 6: 运行新增测试确认失败**

在仓库根目录执行：

```text
cargo test -p adscope-center --test oidc_contract multi_issuer -- --nocapture
cargo test -p adscope-center --test oidc_contract unknown_and_missing_hosts -- --nocapture
cargo test -p adscope-center --test oidc_contract authorization_code_issuer_must_match -- --nocapture
cargo test -p adscope-center --test oidc_contract access_token_is_rejected_by_another_issuer -- --nocapture
```

Expected: 编译失败或断言失败，证明路由仍使用单一 issuer。

- [ ] **Step 7: 在路由入口解析当前 issuer**

`center/src/oidc/routes.rs` 新增：

```rust
fn request_issuer<'a>(state: &'a AppState, headers: &HeaderMap) -> Option<&'a str> {
    let mut values = headers.get_all(header::HOST).iter();
    let value = values.next()?;
    if values.next().is_some() {
        return None;
    }
    state
        .oidc
        .config()
        .issuer_for_host(value.to_str().ok())
}

fn issuer_not_found() -> Response {
    let mut response = StatusCode::NOT_FOUND.into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    response
}
```

所有 OIDC handler 在解析请求内容、Cookie 或 Bearer token 之前调用 `request_issuer`：

- `openid_configuration`、`jwks`、`authorize`、`authorization_context`、`confirm_authorization`、`token`、`token_method_not_allowed`、`userinfo`。
- 目标状态无 Host、Host 不在白名单或存在多个 Host 时返回 `404`。
- 旧 `OIDC_ISSUER` 模式由 `issuer_for_host` 内部直接返回单 issuer，不要求 Host。

discovery 使用：

```rust
let issuer = request_issuer(&state, &headers).ok_or(OidcRouteError::NotFound)?;
public_metadata_json(OpenIdConfiguration {
    issuer: issuer.to_string(),
    authorization_endpoint: format!("{issuer}/oauth2/authorize"),
    token_endpoint: format!("{issuer}/oauth2/token"),
    userinfo_endpoint: format!("{issuer}/oauth2/userinfo"),
    jwks_uri: format!("{issuer}/oauth2/jwks"),
    response_types_supported: ["code"],
    grant_types_supported: ["authorization_code"],
    subject_types_supported: ["public"],
    id_token_signing_alg_values_supported: ["RS256"],
    scopes_supported: ["openid", "profile", "email", "phone"],
    token_endpoint_auth_methods_supported: ["client_secret_basic", "none"],
    code_challenge_methods_supported: ["S256"],
})
```

为 `OidcRouteError` 增加：

```rust
NotFound,
```

并在 `IntoResponse` 中映射为 `404`、`no-store`。`authorize` 一类的 `Response` handler 直接返回 `issuer_not_found()`。

非 POST 的 token fallback 也必须先做同样的 Host 判断：

```rust
async fn token_method_not_allowed(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if request_issuer(&state, &headers).is_none() {
        return issuer_not_found();
    }
    TokenError::invalid_request(StatusCode::METHOD_NOT_ALLOWED).into_response()
}
```

- [ ] **Step 8: 授权码签发和兑换绑定 issuer**

`approve_authorization` 增加 `issuer: &str` 参数，写入：

```rust
let record = AuthorizationCodeRecord {
    code_hash: sha256_token(&code),
    issuer: issuer.to_string(),
    client_id: validated.client.client_id,
    employee_id: session.employee_id,
    redirect_uri: validated.request.redirect_uri.clone(),
    scopes: validated.scopes,
    nonce: validated.request.nonce.clone(),
    code_challenge: validated.request.code_challenge.clone(),
    auth_time,
    expires_at,
};
```

`confirm_authorization` 在调用 `approve_authorization` 之前取当前 issuer。`token` 把当前 issuer 传入 `exchange_token`，在授权码已经从数据库原子取出之后检查：

```rust
let record = exchange.code;
if record.issuer != issuer {
    return Err(TokenError::invalid_grant());
}
```

这个检查必须放在删除授权码之后，保证失败的跨 issuer 兑换不会留下可重试授权码。

- [ ] **Step 9: Token 和 UserInfo 使用当前 issuer**

Token 兑换成功路径改为：

```rust
let access_token = state
    .oidc
    .issue_access_token_for_issuer(issuer, &user.employee_id, &client.client_id, &scope)
    .map_err(|_| TokenError::server_error())?;
let id_token = state
    .oidc
    .issue_id_token_with_user_claims_for_issuer(
        issuer,
        &user.employee_id,
        &client.client_id,
        auth_time,
        &record.nonce,
        user_claims,
    )
    .map_err(|_| TokenError::server_error())?;
```

UserInfo 改为：

```rust
let claims = state
    .oidc
    .verify_access_token_for_issuer(issuer, token)
    .map_err(|_| OidcRouteError::InvalidToken)?;
```

其中 `issuer` 来自 `request_issuer(&state, &headers)`。

- [ ] **Step 10: 更新构造器和现有调用点**

`center/src/state.rs`：

- `new_for_tests` 和 `with_batch_limit_for_tests` 继续表示过渡期的单 issuer 测试，内部调用 `OidcConfig::new_single_issuer_legacy`。
- 新增 `new_for_tests_with_issuers(repository, key, issuers, pem)`，内部调用 `OidcConfig::from_issuers`。
- 测试辅助 `oidc_service_for_tests` 同步改为 legacy 构造；再增加多 issuer 版本的辅助函数。

多 issuer 测试构造器：

```rust
pub fn new_for_tests_with_issuers(
    repository: Repository,
    password_encryption_key: impl Into<String>,
    oidc_issuers: &[&str],
    oidc_private_key_pem: &[u8],
) -> Self {
    Self::with_password_providers(
        repository,
        DEFAULT_BATCH_LIMIT,
        Arc::new(BuiltInPasswordEncryption::new(password_encryption_key)),
        Arc::new(DeterministicPasswordHash),
        UserSessionIssuer::for_tests("test-user-session-key"),
        "test-management-token".to_string(),
        oidc_service_for_tests_with_issuers(oidc_issuers, oidc_private_key_pem),
    )
}

fn oidc_service_for_tests_with_issuers(
    issuers: &[&str],
    private_key_pem: &[u8],
) -> OidcService {
    let config = OidcConfig::from_issuers(issuers, private_key_pem.to_vec(), false)
        .expect("test OIDC configuration must be valid");
    OidcService::new(config).expect("test OIDC private key must be valid")
}
```

更新所有 `OidcConfig::new(` 调用和所有旧的 `issue_*`、`verify_*` 调用，分别改为 legacy 构造或显式 issuer 方法。可以用：

```text
rg -n "OidcConfig::new|issue_id_token|issue_access_token|verify_id_token|verify_access_token|config\(\)\.issuer\(\)" center
```

确认没有全局 issuer 调用残留。

- [ ] **Step 11: 增加环境变量启动测试**

在 `center/tests/api_contract.rs` 中：

- `clear_server_env` 增加 `OIDC_ISSUERS`。
- `set_oidc_env` 保持设置 `OIDC_ISSUER`，用于 legacy 测试。
- 新增 `set_oidc_issuers_env` 设置：

```text
OIDC_ISSUERS=https://auth-a.example.com,https://auth-b.example.com,https://auth-c.example.com
```

- 新增 `app_state_from_env_accepts_oidc_issuers`，断言 `issuer_for_host(Some("auth-a.example.com"))` 返回区域 A issuer。
- 新增 `app_state_from_env_rejects_both_oidc_settings`，同时设置两个变量，断言启动失败且错误包含两个变量名。
- 现有 `app_state_from_env_rejects_invalid_oidc_issuer` 和 `app_state_from_env_requires_oidc_issuer` 改为覆盖 `OIDC_ISSUERS` 的非法格式和缺失情况。
- 现有单 issuer 测试保留，并改名为 legacy 场景或明确注释其验证的是过渡模式。

- [ ] **Step 12: 运行 Center 全量测试和静态检查**

在仓库根目录执行：

```text
cargo fmt --all
cargo test -p adscope-center --test oidc_contract
cargo test -p adscope-center --test api_contract
cargo test -p adscope-center
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: 所有 Center 测试通过，clippy 无警告。若 Cargo 因现有运行进程或文件锁失败，保留阻塞结论，不终止用户进程，也不使用 `CARGO_TARGET_DIR` 绕开。

- [ ] **Step 13: 提交 Core 变更（仅在用户授权提交后）**

```text
git add center/src/oidc/config.rs center/src/oidc/mod.rs center/src/oidc/routes.rs center/src/state.rs center/tests/api_contract.rs center/tests/oidc_contract.rs
git commit -m "支持 Core 按 Portal issuer 提供 OIDC"
```

## Portal PUBLIC_ISSUER、Host 覆盖与监听模式

**Files:**
- Modify: `gateway/src/lib.rs`
- Modify: `gateway/src/main.rs`
- Modify: `gateway/Cargo.toml`（仅当测试需要补 dev dependency 时）
- Modify: `gateway/README.md`

**Interfaces:**
- Consumes: Core 已按 Host 选择 issuer。
- Produces:
  - `Config.public_issuer: PublicIssuer`
  - `Config.tls: ListenerTls`
  - 转发请求固定使用 `PublicIssuer::host()` 覆盖 `Host`

- [ ] **Step 1: 写 PUBLIC_ISSUER 解析和 TLS 组合的失败测试**

在 `gateway/src/lib.rs` 的测试模块增加：

```rust
#[test]
fn public_issuer_is_normalized_and_rejects_invalid_origins() {
    assert_eq!(
        PublicIssuer::parse("https://auth-a.example.com.:443")
            .unwrap()
            .origin(),
        "https://auth-a.example.com"
    );
    assert_eq!(
        PublicIssuer::parse("https://auth-a.example.com.:443")
            .unwrap()
            .host(),
        "auth-a.example.com"
    );

    for value in [
        "http://auth-a.example.com",
        "https://auth-a.example.com/path",
        "https://auth-a.example.com?query=value",
        "https://auth-a.example.com#fragment",
        "https://user@auth-a.example.com",
    ] {
        assert!(PublicIssuer::parse(value).is_err(), "{value}");
    }
}

#[test]
fn tls_files_must_be_configured_together() {
    assert_eq!(listener_tls(None, None).unwrap(), ListenerTls::Http);
    assert_eq!(
        listener_tls(Some("cert.pem".into()), Some("key.pem".into())).unwrap(),
        ListenerTls::Https {
            cert_file: "cert.pem".into(),
            key_file: "key.pem".into(),
        }
    );
    assert!(listener_tls(Some("cert.pem".into()), None).is_err());
    assert!(listener_tls(None, Some("key.pem".into())).is_err());
}
```

再加环境变量缺失测试：

```rust
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn clear_gateway_env() {
    unsafe {
        for name in [
            "CENTER_URL",
            "WEB_ROOT",
            "PUBLIC_ISSUER",
            "TLS_CERT_FILE",
            "TLS_KEY_FILE",
            "LOGIN_ENABLED",
            "ADMIN_ENABLED",
        ] {
            std::env::remove_var(name);
        }
    }
}

fn set_gateway_env(web_root: &std::path::Path, public_issuer: Option<&str>) {
    unsafe {
        std::env::set_var("CENTER_URL", "https://core.example.com");
        std::env::set_var("WEB_ROOT", web_root);
        match public_issuer {
            Some(value) => std::env::set_var("PUBLIC_ISSUER", value),
            None => std::env::remove_var("PUBLIC_ISSUER"),
        }
    }
}

#[test]
fn config_requires_public_issuer() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_gateway_env();
    let web_root = test_web_root();
    set_gateway_env(&web_root, None);

    let error = Config::from_env().unwrap_err();
    assert!(error.to_string().contains("PUBLIC_ISSUER is required"));

    clear_gateway_env();
    std::fs::remove_dir_all(web_root).ok();
}
```

环境变量测试统一使用模块级 `ENV_LOCK`，避免与现有测试并行互相污染；`LOGIN_ENABLED` 和 `ADMIN_ENABLED` 使用默认值。

- [ ] **Step 2: 运行 Portal 测试确认失败**

在仓库根目录执行：

```text
cargo test -p adscope-gateway public_issuer_is_normalized -- --nocapture
cargo test -p adscope-gateway tls_files_must_be_configured_together -- --nocapture
cargo test -p adscope-gateway config_requires_public_issuer -- --nocapture
```

Expected: 编译失败，提示 `PublicIssuer`、`ListenerTls` 或 `PUBLIC_ISSUER` 相关能力不存在。

- [ ] **Step 3: 实现 PublicIssuer 和监听配置**

在 `gateway/src/lib.rs` 增加：

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicIssuer {
    origin: String,
    host: String,
}

impl PublicIssuer {
    fn parse(value: &str) -> anyhow::Result<Self> {
        let value = value.trim();
        let url = Url::parse(value).context("PUBLIC_ISSUER must be an absolute URL")?;
        if url.scheme() != "https" {
            bail!("PUBLIC_ISSUER must use HTTPS");
        }
        if url.host().is_none() {
            bail!("PUBLIC_ISSUER must include a host");
        }
        if !url.username().is_empty() || url.password().is_some() {
            bail!("PUBLIC_ISSUER must not include userinfo");
        }
        if url.query().is_some() {
            bail!("PUBLIC_ISSUER must not include a query");
        }
        if url.fragment().is_some() {
            bail!("PUBLIC_ISSUER must not include a fragment");
        }
        if url.path() != "/" {
            bail!("PUBLIC_ISSUER path must be '/' or empty");
        }

        let host = url
            .host_str()
            .expect("validated PUBLIC_ISSUER has a host")
            .trim_end_matches('.')
            .to_ascii_lowercase();
        if host.is_empty() {
            bail!("PUBLIC_ISSUER host must not be empty");
        }
        let mut normalized = url;
        normalized
            .set_host(Some(&host))
            .expect("validated PUBLIC_ISSUER host must remain valid");
        normalized.set_path("");
        let origin = normalized.as_str().trim_end_matches('/').to_string();
        let host = origin
            .strip_prefix("https://")
            .expect("validated PUBLIC_ISSUER uses HTTPS")
            .to_string();
        Ok(Self { origin, host })
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn host(&self) -> &str {
        &self.host
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListenerTls {
    Http,
    Https {
        cert_file: String,
        key_file: String,
    },
}

fn listener_tls(
    cert_file: Option<String>,
    key_file: Option<String>,
) -> anyhow::Result<ListenerTls> {
    match (cert_file, key_file) {
        (None, None) => Ok(ListenerTls::Http),
        (Some(cert_file), Some(key_file)) => Ok(ListenerTls::Https {
            cert_file,
            key_file,
        }),
        _ => bail!("TLS_CERT_FILE and TLS_KEY_FILE must be set together"),
    }
}
```

`Config` 增加：

```rust
pub public_issuer: PublicIssuer,
pub tls: ListenerTls,
```

`Config::from_env`：

- 读取 `PUBLIC_ISSUER`，缺失时返回 `PUBLIC_ISSUER is required`。
- 读取 `TLS_CERT_FILE` / `TLS_KEY_FILE`，调用 `listener_tls`。
- 不再要求 TLS 文件必须存在。

`AppState` 增加：

```rust
public_issuer_host: String,
```

`AppState::new` 写入 `config.public_issuer.host().to_string()`；测试构造函数使用一个固定的合法公开 issuer。

- [ ] **Step 4: 写 Host 覆盖的失败测试**

在 `gateway/src/lib.rs` 增加一个本地 mock Center：

```rust
async fn mock_center() -> (Url, tokio::task::JoinHandle<()>) {
    let app = Router::new().fallback(|headers: HeaderMap| async move {
        Json(json!({
            "host": headers
                .get(header::HOST)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
        }))
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (Url::parse(&format!("http://{address}")).unwrap(), handle)
}

#[tokio::test]
async fn forwarded_requests_override_client_host_with_public_issuer() {
    let (center_url, handle) = mock_center().await;
    let web_root = test_web_root();
    let config = Config {
        listen_addr: "127.0.0.1:0".parse().unwrap(),
        center_url,
        public_issuer: PublicIssuer::parse("https://auth-a.example.com").unwrap(),
        login_enabled: true,
        admin_enabled: true,
        tls: ListenerTls::Http,
        web_root: web_root.clone(),
    };
    let response = router(AppState::new(&config).unwrap())
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .header(header::HOST, "attacker.example.test")
                .header("x-forwarded-host", "attacker.example.test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["host"], "auth-a.example.com");

    handle.abort();
    std::fs::remove_dir_all(web_root).ok();
}
```

- [ ] **Step 5: 实现 Host 覆盖**

`forward` 构造 upstream 请求时，跳过客户端 `Host`，并在循环后固定追加：

```rust
let mut upstream = state.client.request(method, upstream_url);
for (name, value) in request.headers() {
    if name.as_str() == "host" || is_hop_by_hop(name) {
        continue;
    }
    upstream = upstream.header(name, value);
}
upstream = upstream.header(header::HOST, &state.public_issuer_host);
```

不要读取客户端 `Host` 或 `X-Forwarded-Host` 来决定 issuer。`X-Forwarded-Host` 即使被复制到 Core，也不参与 issuer 选择。

- [ ] **Step 6: 实现 HTTP/HTTPS 启动分支**

把 `gateway/src/main.rs` 的 import 改为：

```rust
use adscope_gateway::{AppState, Config, ListenerTls, router};
```

`gateway/src/main.rs` 根据 `config.tls` 分支：

```rust
let state = AppState::new(&config)?;
let listen_addr = config.listen_addr;

match config.tls.clone() {
    ListenerTls::Https {
        cert_file,
        key_file,
    } => {
        let tls = RustlsConfig::from_pem_file(&cert_file, &key_file)
            .await
            .context("failed to load TLS certificate or key")?;
        axum_server::bind_rustls(listen_addr, tls)
            .serve(router(state).into_make_service())
            .await?;
    }
    ListenerTls::Http => {
        axum_server::bind(listen_addr)
            .serve(router(state).into_make_service())
            .await?;
    }
}
```

只在 HTTPS 分支加载证书；只配置一个 TLS 文件时 `Config::from_env` 已经返回错误，不进入监听。

- [ ] **Step 7: 运行 Portal 测试和静态检查**

在仓库根目录执行：

```text
cargo fmt --all
cargo test -p adscope-gateway
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: Gateway 单元测试通过，Host 覆盖断言得到 `auth-a.example.com`，TLS 组合测试覆盖 HTTP、HTTPS 和单文件失败三种情况。

- [ ] **Step 8: 提交 Portal 变更（仅在用户授权提交后）**

```text
git add gateway/src/lib.rs gateway/src/main.rs gateway/Cargo.toml gateway/README.md
git commit -m "让 Portal 声明公开 issuer 并支持 HTTP 监听"
```

## 文档与部署配置

**Files:**
- Modify: `README.md`
- Modify: `gateway/README.md`
- Modify: `deploy/center/.env.example`
- Modify: `docs/guide/deployment.md`
- Modify: `docs/guide/security.md`
- Modify: `docs/reference/oidc-integration.md`
- Modify: `docs/reference/security-boundary.md`
- Modify: `docs/reference/system-architecture.md`
- Modify: `docs/reference/api-contract.md`
- Modify: `docs/reference/data-model.md`
- Modify: `docs/reference/images/adscope-network-architecture.svg`
- Regenerate: `docs/reference/images/adscope-network-architecture.png`

**Interfaces:**
- Consumes: 已实现的 Core 多 issuer、Portal `PUBLIC_ISSUER` 和 HTTP/HTTPS 监听。
- Produces: 与代码一致的部署变量、拓扑、错误行为、数据库字段和迁移顺序。

- [ ] **Step 1: 更新部署变量**

`deploy/center/.env.example` 改为：

```text
# Core 接受的 Portal issuer 白名单。不要加入 core.example.com。
OIDC_ISSUERS=https://auth-a.example.com,https://auth-b.example.com,https://auth-c.example.com
# 旧部署过渡期才使用 OIDC_ISSUER；它不能与 OIDC_ISSUERS 同时设置。
OIDC_LOOPBACK_HTTP=false
```

`gateway/README.md` 增加：

```text
PUBLIC_ISSUER=https://auth-a.example.com
```

并说明：

- `PUBLIC_ISSUER` 必填，必须是归一化后的 HTTPS origin。
- `TLS_CERT_FILE` 与 `TLS_KEY_FILE` 同时设置时监听 HTTPS。
- 两者都不设置时监听 HTTP，交给前置反向代理终止 TLS。
- 只设置一个时启动失败。

- [ ] **Step 2: 重写单 issuer 相关说明**

所有文档统一改为三个 Portal 各自 issuer，Core 只做后端：

- `README.md` 的“统一登录”和部署说明改为 `OIDC_ISSUERS`，不再说依赖方使用 canonical issuer。
- `gateway/README.md` 的三网关表格改为本文固定的三个域名和外网 `ADMIN_ENABLED=false`。
- `docs/guide/deployment.md` 的 Center 变量、OIDC 私钥、网络接入和部署顺序全文对齐多 issuer。
- `docs/guide/security.md` 删除“Center 只使用一个 canonical issuer”和“issuer 不随入口域名变化”。
- `docs/reference/oidc-integration.md` 删除“多个 issuer 不支持”，写明每个 Portal 是自己的 issuer、客户端记录全局、client 由依赖方选择 Portal。
- `docs/reference/security-boundary.md` 写明 Host 白名单、`core.example.com` OIDC 404、Portal 覆盖 Host、跨 issuer 拒绝。
- `docs/reference/system-architecture.md` 写明 Core / Portal / Syncer 拓扑，Syncer 直连 Core，Portal 按网络划分、Syncer 按 AD 域划分。
- `docs/reference/api-contract.md` 写明 discovery 和 endpoints 随请求 Host 变化、无 Host/未知 Host 返回 404、access token `aud` 为当前 issuer 的 UserInfo。
- `docs/reference/data-model.md` 在 `oauth_authorization_codes` 表中增加 `issuer`，并说明旧记录 `issuer=''`、兑换失败、120 秒自然清空。
- `docs/reference/images/adscope-network-architecture.svg` 的 `desc`、副标题、三个 Portal 域名、Center 说明和底部注释同步改为三个 Portal 各自 issuer；不要修改四域和四 Syncer 的拓扑。
- `docs/reference/images/adscope-network-architecture.png` 用 SVG 重新渲染，不能手改二进制。当前机器没有 `magick`、`inkscape` 或 `rsvg-convert`；执行该任务时需要可用渲染器，否则保留旧 PNG 并把它列为验证缺口，不能声称架构图已更新。

- [ ] **Step 3: 写入部署和迁移顺序**

在 `docs/guide/deployment.md` 增加与 spec 相同的顺序：

```text
1. Core 升级到支持 OIDC_ISSUERS 的版本，启动时执行授权码表补列
2. Core 配置三个 Portal issuer，确认 core.example.com 的 OIDC 路径返回 404
3. Portal 逐个升级，配置 PUBLIC_ISSUER，按实际部署选择 HTTP 或 HTTPS 监听
4. 前置反向代理按域名整体转发到 Portal，不按 /api、/oauth2 拆路径
5. 依赖方把 issuer 从 core.example.com 改为所属 Portal 地址
6. 验证三个 Portal 的 discovery、登录、Token 和 UserInfo
7. 验证四个 Syncer 的同步状态
8. 稳定后移除旧的 OIDC_ISSUER 兼容配置
```

同时写明：

- 外网 Portal `ADMIN_ENABLED=false`。
- 区域 A、内网入口 B Portal `ADMIN_ENABLED=true`。
- 三个 Portal 独立登录，不共享 Cookie，不做跨 Portal SSO。
- 三个 issuer 共用 Core 的一把 RSA 私钥，JWKS 相同。
- Core 只允许 Portal、Syncer 和受信反向代理访问。
- 内网 HTTP 监听只能在受信网络或本机回环使用。

- [ ] **Step 4: 检查文档没有遗留旧结论**

执行：

```text
rg -n "canonical issuer|只有一个 issuer|不支持.*多个 issuer|不随入口域名变化|issuer 不随|issuer 固定|issuer 统一" README.md docs gateway/README.md deploy
```

Expected: 只允许出现在明确标注为旧版本迁移背景、且已说明目标状态的段落；不能把“Center 单 issuer”或“issuer 不随入口域名变化”写成当前实现。

再检查配置变量：

```text
rg -n "OIDC_ISSUER|OIDC_ISSUERS|PUBLIC_ISSUER" README.md docs gateway/README.md deploy
```

Expected: 当前部署示例只使用 `OIDC_ISSUERS` 和 `PUBLIC_ISSUER`；`OIDC_ISSUER` 只出现在过渡兼容说明中。

- [ ] **Step 5: 提交文档变更（仅在用户授权提交后）**

```text
git add README.md gateway/README.md deploy/center/.env.example docs/guide/deployment.md docs/guide/security.md docs/reference/oidc-integration.md docs/reference/security-boundary.md docs/reference/system-architecture.md docs/reference/api-contract.md docs/reference/data-model.md docs/reference/images/adscope-network-architecture.svg docs/reference/images/adscope-network-architecture.png
git commit -m "更新多 issuer 部署说明"
```

## 整体验证与部署验收

**Files:**
- Verify: `Cargo.toml`
- Verify: `crates/store/Cargo.toml`
- Verify: `crates/protocol/Cargo.toml`
- Verify: `gateway/web/package.json`

**Interfaces:**
- Consumes: 前四组任务的全部交付。
- Produces: 可复现的本地验证结果和部署验收清单；不产生 release 镜像。

- [ ] **Step 1: 运行 root workspace 的 Rust 检查**

在仓库根目录执行：

```text
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p adscope-center
cargo test -p adscope-gateway
cargo test -p adscope-connector
```

Expected:

- `cargo fmt --all` 不产生未格式化差异。
- clippy 无 warning。
- Center 的 OIDC、API 契约测试全部通过。
- Gateway 的配置、Host 覆盖和静态文件测试全部通过。
- Connector 的现有契约测试通过，证明共享 Store 变更没有破坏 Syncer 编译和协议行为。

- [ ] **Step 2: 分别验证被 root workspace 排除的 crate**

在 `crates/store` 下执行：

```text
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

在 `crates/protocol` 下执行：

```text
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

Expected: store 的授权码 issuer 往返、旧表补列和现有 OAuth 测试通过；protocol 的现有契约测试通过。

- [ ] **Step 3: 处理前端验证边界**

本计划不修改 `gateway/web` 源码，也不把 `gateway/web/dist` 当作发布输入，因此不因本次改动执行 Nuxt 构建，避免在空间紧张的 E 盘生成新的本地产物。

如果实现过程中确实修改了 `gateway/web`，必须在 `gateway/web` 下执行：

```text
bun run typecheck
bun test
bun run build
```

并且只有在确认 `gateway/static` 的发布产物也需要更新时，才生成和提交静态产物；不得把 `gateway/web/dist` 直接作为发布输入。

- [ ] **Step 4: 在可获得 Docker 和真实网络时执行部署验证**

按以下顺序逐项验收，不能用单元测试代替：

1. 备份 `data/adscope.db` 和 OIDC 私钥。
2. 部署 Core，设置 `OIDC_ISSUERS`，确认启动成功且授权码表存在 `issuer` 列。
3. 从 Portal 网络请求：

```text
curl -fsS https://auth-a.example.com/.well-known/openid-configuration
curl -fsS https://auth-b.example.com/.well-known/openid-configuration
curl -fsS https://auth-c.example.com/.well-known/openid-configuration
```

Expected: 三个响应分别以自身 issuer 作为 `issuer`，端点为对应 Portal 地址。

4. 直接请求 Core：

```text
curl -sS -o /dev/null -w "%{http_code}\n" https://core.example.com/.well-known/openid-configuration
curl -sS -o /dev/null -w "%{http_code}\n" https://core.example.com/oauth2/jwks
```

Expected: 两次都是 `404`。

5. 逐个升级 Portal，配置 `PUBLIC_ISSUER`，确认：
   - 未配置 TLS 文件时 Portal 以 HTTP 启动并可由前置代理访问。
   - 同时配置证书和私钥时以 HTTPS 启动。
   - 只配置一个 TLS 文件时启动失败。
   - 转发到 Core 的 Host 是 `PUBLIC_ISSUER` 的 host，与客户端 Host 无关。
6. 验证三个 Portal 各自登录、各自退出、Discovery、Token、UserInfo，不共享 Cookie。
7. 使用跨 Portal 的授权码和 access token 验证：兑换返回 `invalid_grant`，UserInfo 返回 `401 invalid_token`。
8. 验证外网 Portal 的 `/admin` 返回 `403`，区域 A和内网入口 B Portal 的管理入口正常。
9. 验证四个 Syncer 的同步 revision 正常推进，并确认它们仍直接访问 Core。

- [ ] **Step 5: 记录验证缺口**

最终汇报必须区分：

- 已通过的本地命令和测试。
- 未执行的 Docker 镜像构建。
- 未执行的线上部署、真实 DNS/TLS、反向代理和 Syncer 验收。
- 已发布 `0.2.0` 镜像 UID 与当前源码 UID 的差异不属于本次改动。

不得把“测试通过”表述为“三个 Portal 已完成线上验收”，也不得把计划中的 curl 命令写成已执行结果。

## 计划自检

- [x] Spec 的 issuer 白名单、Host 归一化、`core.example.com` 404、授权码隔离、access token audience 和 UserInfo 隔离都有对应测试。
- [x] Portal 的 `PUBLIC_ISSUER`、Host 覆盖、HTTP/HTTPS 监听和单 TLS 文件失败都有对应测试。
- [x] 数据库新表和旧库补列都有测试，旧授权码没有 issuer 时兑换失败。
- [x] 没有把跨 Portal SSO、独立签名密钥、客户端 issuer 绑定或代码重命名纳入任务。
- [x] 没有把 Docker 构建、推送、打 tag 或线上发布写成已授权或已完成。
