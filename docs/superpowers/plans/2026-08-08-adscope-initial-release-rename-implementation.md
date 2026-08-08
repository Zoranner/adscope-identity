# Adscope Initial Release Rename Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将未发布的 ADSS 原型完整命名为 Adscope，统一所有运行时标识，并以 `v0.1.0` 作为唯一首发版本。

**Architecture:** 产品名、Rust package、环境变量、HTTP 协议、Cookie、服务和发布工件使用同一个 `adscope` 根标识。由于不存在已发布兼容性承诺，旧 `ADSS_*` 配置直接报错，旧会话、密码密文和 Connector state 不迁移。仓库远端名称不在本地 Git 历史中伪造修改，待配置 remote 后在 SCM 中改名。

**Tech Stack:** Rust 2024、Axum、SeaORM、Nuxt 4、Vue 3、Bun、PowerShell 7、Docker、Git。

---

## 文件职责

- `Cargo.toml`、`center/Cargo.toml`、`connector/Cargo.toml`、`crates/*/Cargo.toml`、`Cargo.lock`：首发版本和 Rust package/binary 名。
- `center/src/**`、`connector/src/**`、`crates/**`：环境变量、Cookie、HTTP header、密码/会话用途前缀与 crate imports。
- `center/tests/**`、`connector/tests/**`、`crates/**/tests/**`：新标识的协议与拒绝旧配置的回归覆盖。
- `center/web/**`：页面标题、浏览器请求 header 和展示文本。
- `deploy/**`、`Dockerfile`、`scripts/**`、`docs/guide/**`、`docs/reference/**`、`README.md`：容器、服务、发布包、管理员配置与用户文档。
- `docs/superpowers/specs/2026-08-08-adscope-initial-release-rename-design.md`：已确认的改名契约；历史设计和评审文档不改写。

### Task 1: 先锁定 `0.1.0` 发布工件契约

**Files:**
- Modify: `center/Cargo.toml`, `connector/Cargo.toml`, `crates/protocol/Cargo.toml`, `crates/store/Cargo.toml`, `Cargo.lock`
- Modify: `scripts/build-release.ps1`, `scripts/test-release-contract.ps1`

- [ ] **Step 1: 把发布契约改为新的失败预期**

在 `scripts/test-release-contract.ps1` 中将版本和产物断言替换为：

```powershell
if ($actualVersion -ne '0.1.0') { throw "Unexpected workspace version: $actualVersion" }
$fakeBinary = Join-Path $assemblyRoot 'fake-adscope-connector.exe'
$connectorArchive = Join-Path $assemblyRoot 'adscope-connector-v0.1.0-windows-x86_64.zip'
$centerArchive = Join-Path $assemblyRoot 'adscope-center-v0.1.0-linux-amd64.tar'
```

运行：

```text
pwsh -NoProfile -File scripts/test-release-contract.ps1
```

预期：失败，当前脚本仍读取 `adss-*` package 和 `0.2.0-rc.1`。

- [ ] **Step 2: 统一 package、二进制和发布脚本**

将四个 crate 版本设为 `0.1.0`，并将 package 名改为 `adscope-center`、`adscope-connector`、`adscope-protocol`、`adscope-store`。在 `build-release.ps1` 中将 `Get-AdssVersion` 改为 `Get-AdscopeVersion`，使用下列唯一产物名和 Docker tag：

```powershell
"adscope-connector-v$RequestedVersion-windows-x86_64.zip"
"adscope-center-v$RequestedVersion-linux-amd64.tar"
$imageTag = "adscope-center:$RequestedVersion"
cargo build --release --locked -p adscope-connector
```

Connector ZIP 的可执行文件条目固定为 `adscope-connector.exe`。

- [ ] **Step 3: 验证发布组装契约**

运行：

```text
pwsh -NoProfile -File scripts/test-release-contract.ps1
```

预期：输出 `Release assembly contract passed.`，并检查 manifest 固定为 `0.1.0`。

- [ ] **Step 4: 提交版本和发布边界**

```text
git add Cargo.lock Cargo.toml center/Cargo.toml connector/Cargo.toml crates/protocol/Cargo.toml crates/store/Cargo.toml scripts/build-release.ps1 scripts/test-release-contract.ps1
git commit -m "统一 Adscope 首发版本和发布包名"
```

### Task 2: 迁移 Rust crate 与公共同步协议

**Files:**
- Modify: `center/src/**/*.rs`, `connector/src/**/*.rs`, `crates/protocol/src/lib.rs`, `crates/store/src/**/*.rs`
- Modify: `center/tests/**/*.rs`, `connector/tests/**/*.rs`, `crates/protocol/tests/sync_contract.rs`, `crates/store/tests/**/*.rs`

- [ ] **Step 1: 为新 Connector header 写失败断言**

在 `connector/tests/http_client_contract.rs` 将请求断言改为：

```rust
assert!(request.contains("x-adscope-connector-key: connector-a-key"));
assert!(!request.contains("x-adss-connector-key:"));
```

运行：

```text
cargo test -p adscope-connector --test http_client_contract
```

预期：失败，因为控制平面仍发送旧 header 且 package 仍未改完 imports。

- [ ] **Step 2: 迁移 package imports 和协议根标识**

将所有 `adss_protocol`、`adss_store`、`adss_center`、`adss_connector` imports 和测试 binary 名替换为对应 `adscope_*`。将 Connector 请求 header 改为 `x-adscope-connector-key`，管理 CSRF header 改为 `x-adscope-csrf-token`，所有 Cookie 名改为 `adscope_sso` 与 `adscope_management`。

将密码加密、密码哈希、用户会话、管理会话和 OIDC CSRF 的用途前缀从 `adss:` 改为 `adscope:`，例如：

```rust
const TOKEN_PREFIX: &str = "adscope-user-session:v2";
const MANAGEMENT_TOKEN_PREFIX: &str = "adscope-management-session:v1";
```

所有 URL、测试 fixture、错误响应、Debug 输出和协议参考使用相同新字符串。

- [ ] **Step 3: 验证公共协议**

运行：

```text
cargo test --workspace
cargo test --manifest-path crates/protocol/Cargo.toml
cargo test --manifest-path crates/store/Cargo.toml
```

预期：Center、Connector、protocol 和 store 的测试全数通过；现有测试不再发送或断言 `adss` header、Cookie 或 token 前缀。

- [ ] **Step 4: 提交 Rust 与协议迁移**

```text
git add center connector crates Cargo.lock
git commit -m "迁移 Adscope Rust 包和同步协议标识"
```

### Task 3: 迁移配置并明确拒绝旧环境变量

**Files:**
- Modify: `center/src/config.rs`, `center/src/state.rs`, `center/src/session.rs`, `center/src/password/*.rs`, `center/src/oidc/config.rs`
- Modify: `connector/src/config.rs`, `connector/tests/http_client_contract.rs`, `connector/tests/process_contract.rs`
- Modify: `center/tests/api_contract.rs`, `center/tests/oidc_contract.rs`

- [ ] **Step 1: 增加旧配置拒绝回归**

为 Center 和 Connector 添加测试：仅设置 `ADSS_MANAGEMENT_TOKEN` 或 `ADSS_CENTER_URL` 后启动配置解析时，错误须包含实际旧变量名和 `use ADSCOPE_`。断言新变量可用：

```rust
assert!(error.to_string().contains("ADSS_CENTER_URL is retired; use ADSCOPE_CENTER_URL"));
```

运行：

```text
cargo test -p adscope-connector --test http_client_contract rejects_retired_adss_environment_variables
```

预期：失败，当前配置尚未检查旧变量。

- [ ] **Step 2: 实现一次性配置切换**

将所有运行时读取改为 `ADSCOPE_` 前缀，包括 Center bind/database/password/session/management/OIDC 配置和 Connector center URL/domain/key/interval/LDAP/state 配置。新增集中 `reject_retired_environment_variables()`，枚举旧 `ADSS_` 变量；发现任一变量即返回：

```text
ADSS_CENTER_URL is retired; use ADSCOPE_CENTER_URL
```

将 Docker data 文件改为 `/data/adscope.db`，Connector state/log 名改为 `adscope-connector-state.json` 与 `adscope-connector.log`。不读取旧数据文件、Cookie 或密文。

- [ ] **Step 3: 验证配置和身份边界**

运行：

```text
cargo test -p adscope-center
cargo test -p adscope-connector --test http_client_contract
cargo test -p adscope-connector --test process_contract
```

预期：新 `ADSCOPE_` 配置工作，旧变量给出迁移错误，所有会话和密码/CSRF 断言使用 `adscope:` 前缀。

- [ ] **Step 4: 提交配置切换**

```text
git add center connector Dockerfile
git commit -m "切换 Adscope 配置和运行时状态标识"
```

### Task 4: 更新 Windows 服务、前端和部署工件

**Files:**
- Modify: `connector/src/windows_service.rs`, `connector/src/main.rs`, `deploy/connector/*.ps1`, `deploy/connector/README.md`
- Modify: `center/web/nuxt.config.ts`, `center/web/package.json`, `center/web/bun.lock`, `center/web/app/**/*.vue`, `center/web/app/**/*.ts`
- Modify: `deploy/center/compose.yaml`, `deploy/center/center.env.example`, `connector/.env.example`
- Modify: `scripts/test-connector-service-scripts.ps1`, `scripts/test-docker-contract.ps1`

- [ ] **Step 1: 更新部署脚本失败断言**

将服务和容器契约改为只接受新标识：

```powershell
Assert-Contains $install 'AdscopeConnector' 'fixed service name'
Assert-Contains $compose 'adscope-center:0.1.0' 'Center image name'
Assert-Contains $compose 'adscope-center-data:/data' 'persistent SQLite volume'
```

运行：

```text
pwsh -NoProfile -File scripts/test-connector-service-scripts.ps1
pwsh -NoProfile -File scripts/test-docker-contract.ps1
```

预期：失败，脚本和 Compose 仍包含旧名称。

- [ ] **Step 2: 迁移可见产品名称和部署入口**

将 Windows 服务内部名、显示名和 uninstall 目标统一为 `AdscopeConnector`/`Adscope Connector`。将 Nuxt title、管理壳品牌、README、Compose image/volume、环境示例、安装路径、测试中的 CLI 名和发布脚本中的文件名统一为 `Adscope`/`adscope`。

前端仅替换品牌、Cookie/Header 使用和配置说明；页面路由、数据模型和 API 路径保持不变。Bun lock 的 root package 名同步为 `adscope-center-web`。

- [ ] **Step 3: 验证部署、前端和旧标识扫描**

运行：

```text
pwsh -NoProfile -File scripts/test-connector-service-scripts.ps1
pwsh -NoProfile -File scripts/test-docker-contract.ps1
cd center/web && bun run typecheck && bun run build
rg -n -i --glob '!target/**' --glob '!dist/**' --glob '!center/web/node_modules/**' --glob '!docs/reviews/**' --glob '!docs/superpowers/**' 'ADSS|adss|AD Structure Sync|ADStructureSync' .
```

预期：两份 PowerShell 契约和前端检查通过，最后一条命令无输出。

- [ ] **Step 4: 提交交付层迁移**

```text
git add center/web connector deploy scripts Dockerfile README.md docs
git commit -m "更新 Adscope 部署和用户界面标识"
```

### Task 5: 校验改名后的完整工程

**Files:**
- No planned writes: this task verifies the completed migration.

- [ ] **Step 1: 执行格式化和 Rust 质量门禁**

运行：

```text
cargo fmt --all
cargo fmt --manifest-path crates/protocol/Cargo.toml
cargo fmt --manifest-path crates/store/Cargo.toml
cargo clippy --all-targets --all-features -- -D warnings
cargo clippy --manifest-path crates/protocol/Cargo.toml --all-targets --all-features -- -D warnings
cargo clippy --manifest-path crates/store/Cargo.toml --all-targets --all-features -- -D warnings
```

预期：三个 Clippy 命令均以零 warnings 退出。

- [ ] **Step 2: 执行完整回归和发布契约**

运行：

```text
cargo test --workspace
cargo test --manifest-path crates/protocol/Cargo.toml
cargo test --manifest-path crates/store/Cargo.toml
pwsh -NoProfile -File scripts/test-docker-contract.ps1
pwsh -NoProfile -File scripts/test-connector-service-scripts.ps1
pwsh -NoProfile -File scripts/test-release-contract.ps1
git diff --check
```

预期：全部通过，且不以单元测试替代真实 AD、Kerberos、Docker 或外部 OIDC 验收。

### Task 6: 清理错误本地发布物并生成首发候选

**Files:**
- Delete: local Git tag `v0.2.0-rc.1`
- Delete: `dist/v0.2.0-rc.1`
- Create: `dist/v0.1.0/adscope-connector-v0.1.0-windows-x86_64.zip`, `dist/v0.1.0/manifest.json`, `dist/v0.1.0/SHA256SUMS`

- [ ] **Step 1: 删除已确认错误的本地标签和产物**

先确认目标只存在于本地：

```text
git tag -l v0.2.0-rc.1
Get-ChildItem -LiteralPath dist/v0.2.0-rc.1 -Force
```

然后删除精确目标：

```text
git tag -d v0.2.0-rc.1
Remove-Item -LiteralPath 'dist/v0.2.0-rc.1' -Recurse -Force
```

预期：旧标签和目录均不存在；`dist/v0.1.0` 不受影响。

- [ ] **Step 2: 创建 `v0.1.0` 并构建 Windows 发布候选**

确认工作树干净后执行：

```text
git tag -a v0.1.0 -m "Adscope 0.1.0"
pwsh -NoProfile -File scripts/build-release.ps1 -Version 0.1.0 -SkipDocker
```

预期：生成 `dist/v0.1.0/adscope-connector-v0.1.0-windows-x86_64.zip`，manifest 的 revision 等于该标签指向的 commit，target 为 `windows-x86_64`。

- [ ] **Step 3: 核验首发候选并报告完整发布前提**

使用 `Get-FileHash -Algorithm SHA256` 对照 `SHA256SUMS`，并列出 ZIP 内的 `adscope-connector.exe`、`.env.example`、安装/卸载脚本和 README。检查 `git status --short --branch` 与 `git show v0.1.0`。

完整线上 Release 仅在 Docker 生成 Linux AMD64 Center archive、Git remote 已配置且用户明确允许 push/release 后执行；缺少任一条件时报告为本地 Windows 候选，不称为线上发布。
