# 发布交付收口 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 交付可追溯的 Center Docker 服务、Connector 原生 Windows 服务、发布产物和浏览器验收入口，使 `v0.1.0` 具备进入真实环境验收的工程闭环。

**Architecture:** Center 由锁定的 Bun 和 Cargo 多阶段构建生成非 root Docker 镜像，SQLite 位于独立数据卷，TLS 由现有反向代理终止。Connector 在同一二进制中复用控制台和 Windows 服务运行循环，使用本地滚动日志保存脱敏故障详情，并保持通用失败确认协议不变。发布脚本从干净提交组装镜像归档、Windows ZIP、版本清单和校验文件。

**Tech Stack:** Rust 2024、Axum 0.8、SeaORM 2.0 RC、Tokio、ldap3、reqwest、windows-service 0.8.1、tracing 0.1、tracing-subscriber 0.3.23、tracing-appender 0.2.5、Nuxt 4、Bun 1.3.14、Playwright、Docker、PowerShell 7。

---

## 文件边界

Center 工作单元拥有以下文件：

- `crates/store/src/repository.rs`
- `crates/store/tests/repository_contract.rs`
- `center/src/routes.rs`
- `center/tests/api_contract.rs`
- `Dockerfile`
- `.dockerignore`
- `deploy/center/compose.yaml`
- `deploy/center/center.env.example`
- `scripts/test-docker-contract.ps1`

Connector 工作单元拥有以下文件：

- `Cargo.toml`
- `Cargo.lock`
- `connector/Cargo.toml`
- `connector/src/**`
- `connector/tests/**`
- `deploy/connector/install-service.ps1`
- `deploy/connector/uninstall-service.ps1`
- `deploy/connector/README.md`
- `scripts/test-connector-service-scripts.ps1`

发布工作单元拥有以下文件：

- `.gitignore`
- `scripts/build-release.ps1`
- `scripts/test-release-contract.ps1`

浏览器工作单元拥有以下文件：

- `center/web/package.json`
- `center/web/bun.lock`
- `center/web/playwright.config.ts`
- `center/web/e2e/start-center.ts`
- `center/web/e2e/connector-key.spec.ts`
- `center/web/app/components/admin/AdminShell.vue`
- `center/web/app/components/admin/Modal.vue`
- `center/web/app/components/domains/DomainEditor.vue`

主会话在各工作单元合并后统一修改：

- `README.md`
- `docs/guide/deployment.md`
- `docs/guide/security.md`
- `docs/reference/api-contract.md`
- `center/.env.example`
- `connector/.env.example`

## Center 健康检查

**Files:**

- Modify: `crates/store/src/repository.rs`
- Test: `crates/store/tests/repository_contract.rs`
- Modify: `center/src/routes.rs`
- Test: `center/tests/api_contract.rs`

- [ ] **编写 Repository ping 的失败测试**

在 `repository_contract.rs` 增加真实 SQLite 和断开连接两项断言：

```rust
#[tokio::test]
async fn repository_ping_reports_connection_state() {
    let repository = sqlite_repository().await;
    repository.ping().await.unwrap();

    let disconnected = Repository::from_connection(Default::default());
    assert!(disconnected.ping().await.is_err());
}
```

- [ ] **运行 RED 验证**

```text
cargo test --manifest-path crates/store/Cargo.toml repository_ping_reports_connection_state
```

Expected: 编译失败，提示 `Repository::ping` 不存在。

- [ ] **实现最小数据库探活**

在 `Repository` 中增加：

```rust
pub async fn ping(&self) -> anyhow::Result<()> {
    self.db.execute_unprepared("SELECT 1").await?;
    Ok(())
}
```

- [ ] **运行 GREEN 验证**

```text
cargo test --manifest-path crates/store/Cargo.toml repository_ping_reports_connection_state
```

Expected: 1 passed。

- [ ] **编写 Center 健康接口失败测试**

在 `api_contract.rs` 用 `build_router` 验证：可用 SQLite 返回 `200` 和 `{"status":"ok"}`；`Repository::from_connection(Default::default())` 返回 `503` 和 `{"status":"unavailable"}`；响应不包含密钥或数据库 URL。

- [ ] **运行 RED 验证**

```text
cargo test -p adss-center health_reports_database_readiness
```

Expected: `/api/health` 返回 404。

- [ ] **实现健康接口**

在 `api_routes()` 注册 `get(health)`，使用稳定响应类型：

```rust
#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    match state.repository.ping().await {
        Ok(()) => (StatusCode::OK, Json(HealthResponse { status: "ok" })),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse { status: "unavailable" }),
        ),
    }
}
```

- [ ] **验证 Center 与 store 回归**

```text
cargo test -p adss-center
cargo test --manifest-path crates/store/Cargo.toml
```

Expected: 全部通过。

- [ ] **提交 Center 健康检查**

```text
git add crates/store/src/repository.rs crates/store/tests/repository_contract.rs center/src/routes.rs center/tests/api_contract.rs
git commit -m "增加 Center 健康检查"
```

## Center Docker 交付

**Files:**

- Create: `Dockerfile`
- Create: `.dockerignore`
- Create: `deploy/center/compose.yaml`
- Create: `deploy/center/center.env.example`
- Create: `scripts/test-docker-contract.ps1`

- [ ] **编写 Docker 构建上下文 RED 测试**

创建 `scripts/test-docker-contract.ps1`，检查必需文件和稳定构建约束。测试首先因 Docker 交付文件不存在而失败：

```powershell
$required = @('Dockerfile', '.dockerignore', 'deploy/center/compose.yaml')
$missing = $required | Where-Object { -not (Test-Path -LiteralPath $_) }
if ($missing.Count) { throw "missing Docker delivery files: $($missing -join ', ')" }
```

```powershell
pwsh -NoProfile -File scripts/test-docker-contract.ps1
```

Expected: 报告 `Dockerfile` 等文件缺失。

- [ ] **创建锁定的多阶段 Dockerfile**

Dockerfile 使用以下固定结构，不在构建期安装源码仓库以外的工具：

```dockerfile
FROM oven/bun:1.3.14-debian AS web-build
WORKDIR /src/center/web
COPY center/web/package.json center/web/bun.lock ./
RUN bun install --frozen-lockfile
COPY center/web/ ./
RUN bun run build

FROM rust:1.93.1-bookworm AS rust-build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY center/ center/
COPY connector/ connector/
COPY crates/ crates/
RUN cargo build --release --locked -p adss-center

FROM debian:bookworm-slim
ARG VERSION=0.1.0
ARG REVISION=unknown
LABEL org.opencontainers.image.version=$VERSION \
      org.opencontainers.image.revision=$REVISION
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home /app adss \
    && mkdir -p /app/web /data \
    && chown -R adss:adss /app /data
COPY --from=rust-build /src/target/release/adss-center /app/adss-center
COPY --from=web-build /src/center/web/.output/public/ /app/web/
ENV ADSS_BIND_ADDR=0.0.0.0:8080 \
    ADSS_WEB_ROOT=/app/web \
    ADSS_DATABASE_URL=sqlite:///data/adss.db?mode=rwc
USER 10001:10001
VOLUME ["/data"]
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --retries=3 CMD curl --fail --silent http://127.0.0.1:8080/api/health || exit 1
ENTRYPOINT ["/app/adss-center"]
```

- [ ] **限制 Docker 构建上下文**

`.dockerignore` 必须排除 `.git`、`.env`、`target`、所有 `node_modules`、`.nuxt`、`.output`、`dist`、日志和 IDE 文件，同时不能排除 `crates/protocol`、`crates/store`、`center/web/package.json` 或 `center/web/bun.lock`。

- [ ] **增加 Center Compose 模板**

`compose.yaml` 只定义 Center、内部端口和 SQLite volume，不配置反向代理或证书：

```yaml
services:
  center:
    image: adss-center:0.1.0
    restart: unless-stopped
    env_file:
      - center.env
    volumes:
      - adss-center-data:/data
    expose:
      - "8080"
    read_only: true
    tmpfs:
      - /tmp

volumes:
  adss-center-data:
```

`center.env.example` 只保留变量名和非秘密说明值，不能包含可用于生产的密钥。

- [ ] **运行 GREEN 静态契约验证**

扩展 `test-docker-contract.ps1`，验证 Dockerfile 同时包含 `--frozen-lockfile`、`--locked`、`USER 10001:10001`、`/api/health` 和 `/data`；验证 `.dockerignore` 包含 `.env`、`target`、`node_modules`，且 Compose 不发布宿主端口、不挂载证书。

```powershell
pwsh -NoProfile -File scripts/test-docker-contract.ps1
```

Expected: 所有断言成功。当前环境没有 Docker，不声明镜像构建通过。

- [ ] **提交 Center Docker 交付**

```text
git add Dockerfile .dockerignore deploy/center/compose.yaml deploy/center/center.env.example scripts/test-docker-contract.ps1
git commit -m "增加 Center Docker 交付入口"
```

## Connector 故障与超时

**Files:**

- Modify: `Cargo.toml`
- Modify: `connector/Cargo.toml`
- Modify: `connector/src/config.rs`
- Modify: `connector/src/control_plane.rs`
- Modify: `connector/src/directory/mod.rs`
- Modify: `connector/src/directory/ldap.rs`
- Modify: `connector/src/runtime.rs`
- Modify: `connector/src/lib.rs`
- Test: `connector/tests/execution_contract.rs`
- Test: `connector/tests/http_client_contract.rs`
- Test: `connector/tests/runtime_contract.rs`

- [ ] **编写执行错误保留的失败测试**

把执行结果断言改为检查 `ExecutionResult`：

```rust
let result = execute_directory_plan(&client, &plan, &context).await;
assert_eq!(result.summary.failed, 1);
let failure = result.failure.unwrap();
assert_eq!(failure.operation, "ensure_user");
assert_eq!(failure.subject, "1001");
assert!(failure.detail.contains("LDAPS permission denied"));
```

凭据失败测试必须断言 `subject == "1002"`，并断言 `detail` 不包含该测试凭据明文。

- [ ] **运行 RED 验证**

```text
cargo test -p adss-connector --test execution_contract
```

Expected: `ExecutionResult` 或 `failure` 不存在。

- [ ] **实现执行结果类型**

在 `directory/mod.rs` 增加：

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionFailure {
    pub operation: &'static str,
    pub subject: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionResult {
    pub summary: SyncSummary,
    pub failure: Option<ExecutionFailure>,
}
```

目录操作使用 `DirectoryOperation.subject`，凭据操作只使用 `employee_id`。底层 `anyhow::Error` 只转成 `detail`，不得格式化 `CredentialEntry` 或配置对象。

- [ ] **让 Runtime 暴露双通道失败详情**

在 `ConnectorRunSummary` 增加 `directory_failure` 和 `credential_failure`，保持失败 confirm 的 `directory_execution_failed`、`credential_execution_failed` 不变。更新测试断言：详细错误存在、本地 revision 不推进、Center 只收到通用错误码，另一个通道仍可成功确认。

- [ ] **运行 Runtime GREEN 验证**

```text
cargo test -p adss-connector --test execution_contract
cargo test -p adss-connector --test runtime_contract
```

Expected: 全部通过。

- [ ] **编写配置脱敏和超时 RED 测试**

增加以下行为：

```rust
let debug = format!("{config:?}");
assert!(!debug.contains("connector-a-key"));
assert!(!debug.contains("BindSecret123!"));
assert!(debug.contains("[redacted]"));
```

并验证 `ADSS_CONNECTOR_HTTP_TIMEOUT_SECONDS=0`、`ADSS_CONNECTOR_OPERATION_TIMEOUT_SECONDS=0` 被拒绝，真实 LDAP 模式下 `http://` Center 地址被拒绝，dry-run 允许本机 HTTP。

- [ ] **实现最小配置边界**

配置增加两个正整数秒数，默认均为 60。为 `ConnectorProcessConfig` 和 `LdapDirectoryConfig` 实现手写 `Debug`，秘密字段固定输出 `[redacted]`。真实 LDAP 模式要求 `ADSS_CENTER_URL` 使用 `https://`。

- [ ] **编写 HTTP 超时 RED 测试**

在现有本地 TCP 测试服务中接受连接但不返回响应，使用 50ms 超时创建客户端并断言请求在 1 秒内返回超时错误。

- [ ] **实现 reqwest 和 LDAP 超时**

`HttpControlPlaneClient::new` 改为返回 `anyhow::Result<Self>`，使用：

```rust
let client = reqwest::Client::builder()
    .connect_timeout(timeout)
    .timeout(timeout)
    .build()?;
```

LDAP 连接使用 `LdapConnSettings::set_conn_timeout`。目录和凭据单项操作由 executor 使用 `tokio::time::timeout` 包裹，并把 elapsed 转成不含秘密的错误详情。

- [ ] **运行 Connector 完整回归**

```text
cargo test -p adss-connector
```

Expected: 全部通过。

- [ ] **提交 Connector 故障与超时**

```text
git add Cargo.toml Cargo.lock connector/Cargo.toml connector/src connector/tests
git commit -m "完善 Connector 故障诊断和超时"
```

## Connector 原生 Windows 服务

**Files:**

- Create: `connector/src/cli.rs`
- Create: `connector/src/logging.rs`
- Create: `connector/src/process.rs`
- Create: `connector/src/windows_service.rs`
- Modify: `connector/src/lib.rs`
- Replace: `connector/src/main.rs`
- Modify: `connector/Cargo.toml`
- Modify: `Cargo.lock`
- Create: `connector/tests/process_contract.rs`
- Create: `connector/tests/cli_contract.rs`
- Create: `deploy/connector/install-service.ps1`
- Create: `deploy/connector/uninstall-service.ps1`
- Create: `deploy/connector/README.md`
- Create: `scripts/test-connector-service-scripts.ps1`

- [ ] **读取并核对一手 API**

实现前核对 `windows-service 0.8.1` 的 `service_dispatcher`、`service_control_handler` 和 `ServiceStatusHandle` 示例，以及 `tracing-appender 0.2.5` 的 `rolling::Builder::max_log_files`。不照搬旧版本接口。

- [ ] **编写 CLI RED 测试**

期望 API：

```rust
assert_eq!(
    ConnectorCommand::parse(["adss-connector", "--runtime-dir", r"C:\ADSS"])? ,
    ConnectorCommand::Console { runtime_dir: PathBuf::from(r"C:\ADSS") }
);
assert!(matches!(
    ConnectorCommand::parse(["adss-connector", "--service", "--runtime-dir", r"C:\ADSS"])? ,
    ConnectorCommand::Service { .. }
));
assert_eq!(ConnectorCommand::parse(["adss-connector", "--version"] )?, ConnectorCommand::Version);
```

同时断言缺少 `--runtime-dir` 值和未知参数返回明确错误。

- [ ] **实现 CLI 并验证 GREEN**

默认命令为控制台模式，默认运行目录为当前目录；服务安装脚本必须显式传入绝对运行目录。`--version` 输出 `adss-connector 0.1.0`，不读取 `.env`。

```text
cargo test -p adss-connector --test cli_contract
```

- [ ] **编写可停止运行循环 RED 测试**

使用真实 `ConnectorRuntime` 测试替身和 `tokio::sync::watch`：首次同步立即执行；发送 stop 后不再开始下一轮；正在执行的一轮完成后函数返回。

- [ ] **实现共享运行循环**

`process.rs` 负责加载 config 后构造 HTTP、目录和 state 客户端。控制台和服务只负责提供运行目录、日志模式和 stop receiver。循环结构固定为：立即 `run_once`、记录摘要、`tokio::select!` 等待 interval 或 stop。

- [ ] **编写日志脱敏 RED 测试**

在临时日志目录初始化 file appender，写入包含 `ExecutionFailure` 的同步摘要，刷新 guard 后断言日志包含 operation、subject、detail，同时不包含测试 Connector key、bind password 或密码明文。

- [ ] **实现有限保留的日志**

服务模式使用 `tracing_appender::rolling::Builder` 配置 daily rotation、文件名前缀 `adss-connector.log` 和 `max_log_files(14)`；控制台模式输出到 stderr。进程生命周期持有 `WorkerGuard`，避免退出前丢日志。

- [ ] **实现 Windows SCM 适配层**

仅在 `cfg(windows)` 编译 `windows_service.rs`。固定服务名 `ADStructureSyncConnector`，注册 Stop 控制，报告 `StartPending`、`Running`、`StopPending`、`Stopped`。服务入口切换到运行目录后加载 `.env`，创建 Tokio runtime，并调用共享运行循环。非 Windows 构建不能引用 Windows API。

- [ ] **编写服务脚本 RED 测试**

创建 `scripts/test-connector-service-scripts.ps1`，用 PowerShell Parser API 解析两个目标脚本，并检查安装脚本包含固定服务名、`LocalService`、`--service --runtime-dir`、自动启动和失败恢复，卸载脚本包含停止、删除且不删除 `.env`、state 或 logs。首次运行应因目标脚本不存在而失败。

```powershell
pwsh -NoProfile -File scripts/test-connector-service-scripts.ps1
```

Expected: 报告安装或卸载脚本缺失。

- [ ] **创建安装与卸载脚本**

`install-service.ps1` 必须：要求管理员权限、解析绝对目录、验证 exe 与 `.env`、拒绝覆盖已有服务、向 LocalService SID `S-1-5-19` 授予最小目录权限、注册带 `--service --runtime-dir` 的 binPath、配置自动启动和失败恢复、启动后检查 Running。

`uninstall-service.ps1` 必须：停止服务、等待 Stopped、删除服务，但保留 `.env`、state 和 logs。两个脚本均使用 `$ErrorActionPreference = 'Stop'`，不吞掉 `sc.exe` 错误码。

- [ ] **验证脚本 GREEN 与 Windows 编译**

```powershell
pwsh -NoProfile -File scripts/test-connector-service-scripts.ps1
```

```text
cargo test -p adss-connector
cargo build --release --locked -p adss-connector
```

Expected: PowerShell 无解析错误，Windows release 编译通过。不在开发机注册真实服务。

- [ ] **提交 Windows 服务交付**

```text
git add Cargo.toml Cargo.lock connector deploy/connector scripts/test-connector-service-scripts.ps1
git commit -m "将 Connector 接入 Windows 服务"
```

## 发布产物组装

**Files:**

- Modify: `.gitignore`
- Create: `scripts/build-release.ps1`
- Create: `scripts/test-release-contract.ps1`

- [ ] **编写发布契约 RED 脚本**

`test-release-contract.ps1` 先确认 `build-release.ps1` 不存在并失败。测试随后会 dot-source 发布脚本，只调用纯组装函数，在系统临时目录使用假的 Connector 二进制和真实包内配套文件验证以下归档项：

```powershell
$requiredEntries = @(
  'adss-connector.exe',
  '.env.example',
  'install-service.ps1',
  'uninstall-service.ps1',
  'README.md'
)
```

并验证 `manifest.json` 包含 `version`、`revision`、`target`、`sha256`，`SHA256SUMS` 与实际文件哈希一致。测试还在临时目录初始化一个 Git 仓库，确认 dirty 检查拒绝未提交文件。传入错误 `-Version 9.9.9` 必须在执行构建前失败。

- [ ] **运行 RED 验证**

```powershell
pwsh -NoProfile -File scripts/test-release-contract.ps1
```

Expected: 因发布脚本不存在而失败。

- [ ] **实现结构化发布脚本**

脚本使用 `cargo metadata --no-deps --format-version 1 | ConvertFrom-Json` 读取 Center 和 Connector 版本，拒绝版本不一致和脏工作树。可测试的 `Assert-CleanWorktree`、`Get-AdssVersion`、`New-ConnectorArchive`、`Write-ReleaseManifest` 函数与主入口保留在同一脚本；dot-source 时不执行主入口。Connector 使用 `cargo build --release --locked -p adss-connector`，显式组装 staging 目录并压缩。

未指定 `-SkipDocker` 时执行：

```powershell
docker build --platform linux/amd64 `
  --build-arg "VERSION=$Version" `
  --build-arg "REVISION=$revision" `
  --tag "adss-center:$Version" .
docker save --output $centerArchive "adss-center:$Version"
```

脚本不创建 tag、不 push、不上传文件。`dist/` 加入 `.gitignore`。

- [ ] **运行 GREEN 验证**

```powershell
pwsh -NoProfile -File scripts/test-release-contract.ps1
```

Expected: 纯组装、dirty 拒绝、版本拒绝、Connector ZIP、manifest 和 SHA256 契约通过；测试不调用 Docker，也不要求先提交未验证代码。

- [ ] **提交发布入口**

```text
git add .gitignore scripts/build-release.ps1 scripts/test-release-contract.ps1
git commit -m "增加首版发布产物组装"
```

## 管理端浏览器验收

**Files:**

- Modify: `center/web/package.json`
- Modify: `center/web/bun.lock`
- Create: `center/web/playwright.config.ts`
- Create: `center/web/e2e/start-center.ts`
- Create: `center/web/e2e/connector-key.spec.ts`
- Modify: `center/web/app/components/admin/AdminShell.vue`
- Modify: `center/web/app/components/admin/Modal.vue`
- Modify: `center/web/app/components/domains/DomainEditor.vue`

- [ ] **安装锁定的 Playwright 开发依赖**

```text
bun add --dev @playwright/test
bunx playwright install chromium
```

只允许 Bun 更新 `package.json` 和 `bun.lock`，不得生成 `package-lock.json`。

- [ ] **编写 Connector key 浏览器 RED 测试**

测试使用真实 Center、SQLite 临时数据库和生成后的 Nuxt 静态文件。流程必须覆盖：输入管理凭证、进入域页、新建域、保存期间点击导航不离开、一次性 key 输入获得焦点、复制内容等于输入值、Tab 从最后按钮回到弹窗首个按钮、关闭后 key 不再存在、编辑保存生成不同 key。

关键断言使用可访问名称：

```ts
await page.locator('.credential-card input[type="password"]').fill('test-management-token')
await page.getByRole('button', { name: '进入' }).click()
await page.getByRole('button', { name: '新建' }).click()
await expect(page.getByRole('dialog')).toBeVisible()
await expect(page.getByLabel('ADSS_CONNECTOR_KEY')).toBeFocused()
```

- [ ] **运行 RED 验证**

```text
bun run test:e2e
```

Expected: 测试入口或配置尚不存在而失败。

- [ ] **实现可重复的 E2E 启动器**

`start-center.ts` 使用 `Bun.spawn` 启动 `cargo run --locked -p adss-center`，通过进程环境注入临时 SQLite URL、测试密钥、管理凭证、监听端口和绝对 `ADSS_WEB_ROOT`。退出信号必须转发给子进程，临时数据库位于系统临时目录，不写入仓库。

Playwright 配置先执行 `bun run build`，再通过 `webServer` 启动测试 Center；Chromium context 授予当前 origin 剪贴板权限。测试不得把生成的 key 输出到日志或截图文件名。

同时为管理凭证、域表单字段和弹窗标题补齐稳定的 `for`/`id`、`aria-labelledby` 关联，再把测试定位器收敛为 `getByLabel` 和带名称的 `getByRole('dialog')`。这些可访问关系由浏览器 RED 测试先证明缺失，再写模板实现。

- [ ] **运行 GREEN 与移动视口验证**

```text
bun run typecheck
bun run test:e2e
```

Expected: Chromium 测试通过。另用 390x844 viewport 重跑域创建与 key 展示，断言页面无水平滚动。

- [ ] **提交浏览器验收入口**

```text
git add center/web/package.json center/web/bun.lock center/web/playwright.config.ts center/web/e2e center/web/app/components/admin/AdminShell.vue center/web/app/components/admin/Modal.vue center/web/app/components/domains/DomainEditor.vue
git commit -m "增加 Connector key 浏览器验收"
```

## 部署与恢复文档

**Files:**

- Modify: `README.md`
- Modify: `docs/guide/deployment.md`
- Modify: `docs/guide/security.md`
- Modify: `docs/reference/api-contract.md`
- Modify: `center/.env.example`
- Modify: `connector/.env.example`

- [ ] **更新正式设计与操作说明**

文档使用无序号标题，写清以下稳定契约：Center 镜像和数据卷、反向代理 TLS 边界、`/api/health`、SQLite 停写备份、加密密钥分离保存与联合恢复、Connector 安装/升级/回滚、日志目录与保留、HTTP/LDAP 超时变量、真实模式强制 HTTPS。

- [ ] **更新环境变量示例**

Center Docker 示例使用 `/data/adss.db`，Connector 增加两个 timeout 变量并明确真实模式要求 HTTPS。示例不得包含可用于生产的密钥。

- [ ] **检查文档边界**

```text
rg -n "第一阶段|当前已实现|待补|TBD|TODO" README.md docs center/.env.example connector/.env.example
```

Expected: 无阶段叙事、占位符或待办表达。

- [ ] **提交部署文档**

```text
git add README.md docs/guide/deployment.md docs/guide/security.md docs/reference/api-contract.md center/.env.example connector/.env.example
git commit -m "补充首版部署和恢复说明"
```

## 集成验证

**Files:** 无新增文件。

- [ ] **执行格式与静态检查**

```text
cargo fmt --all
cargo fmt --manifest-path crates/protocol/Cargo.toml
cargo fmt --manifest-path crates/store/Cargo.toml
cargo clippy --all-targets --all-features -- -D warnings
cargo clippy --manifest-path crates/protocol/Cargo.toml --all-targets --all-features -- -D warnings
cargo clippy --manifest-path crates/store/Cargo.toml --all-targets --all-features -- -D warnings
```

Expected: 无 warning、无 `#[allow(...)]` 绕过。

- [ ] **执行完整自动化**

```text
cargo test --workspace
cargo test --manifest-path crates/protocol/Cargo.toml
cargo test --manifest-path crates/store/Cargo.toml
cargo build --release --workspace --locked
bun install --frozen-lockfile
bun run typecheck
bun run build
bun run test:e2e
```

Expected: 全部通过。

- [ ] **执行发布产物验证**

```powershell
pwsh -NoProfile -File scripts/test-release-contract.ps1
```

Expected: Connector ZIP、manifest 和 SHA256 通过。若环境仍无 Docker，明确记录未执行镜像构建，不使用 `-SkipDocker` 的结果冒充完整发布。

- [ ] **复核 diff 与规格覆盖**

```text
git diff --check
git status --short --branch
git log --oneline -n 10
```

逐条对照 `docs/superpowers/specs/2026-08-03-release-closeout-design.md`，确认没有新增 PostgreSQL、CI、证书管理、WinSW/NSSM、管理员实体或独立 key rotation。

- [ ] **收口提交**

若集成修复产生新变更，仅提交这些修复；不 amend 已完成的独立提交，不创建 tag，不推送，不部署。

## 真实环境验收边界

代码和本机自动化完成后，仍需在有权限的目标环境执行以下验收，结果不由本计划虚构：

- 从生成的 Center 镜像归档加载并启动容器，验证数据卷权限、健康检查和反向代理 HTTPS。
- 验证代理不记录或缓存 Connector key 与 `/api/connector/sync` 响应体。
- 安装 Connector Windows 服务，验证 LocalService 权限、启动、停止、失败恢复和日志轮转。
- 停止 Center 后备份 SQLite，使用原加密密钥完成联合恢复。
- 在 AD 沙箱验证 OU、用户、组、成员、禁用、隔离移动、Reset Password、重启和 rebuild。
