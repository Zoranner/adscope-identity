# AD 多域组织结构同步工程评审报告

评审日期：2026-08-07

本报告记录本轮实施前的工程基线，以下问题用于驱动随后的改造，并非发布候选的现状清单。随本次发布候选已处理管理密钥的浏览器持久化、LDAP Simple Authentication、Connector `.env` 继承 ACL、用户与初始凭据的跨事务创建、目录批量分页和产品概览的 OIDC 范围漂移；对应回归由 Rust、PowerShell 和 Nuxt 静态构建重新执行。弱密钥启动校验、前端独立 lint/test 入口、Nuxt/Nitro 构建警告以及真实域环境验收仍未在本轮解决。

## 范围与依据

本次评审覆盖根工作区中的 `center`、`connector`、`crates/protocol`、`crates/store`、`center/web` 以及部署、发布和参考文档。评审依据为仓库 README、技术参考、部署与安全文档、Cargo/Nuxt 配置、现有测试脚本，以及 Rust 与 Nuxt/Vue 工程评审基线。

本次只评估工程边界、凭据处理、可验证性和文档一致性，不评价 AD 域控、TLS 反向代理、真实 OIDC 客户端或生产权限的实际部署效果。

## 结论

项目的模块职责、协议文档、Docker 交付约束和 Connector 服务脚本测试已有清晰基础，但当前不应作为生产就绪状态放行。管理面长期密钥的浏览器存储、弱密钥启动校验、Connector 的明文 LDAP 路径和运行目录 ACL 都会直接扩大凭据暴露面；此外，目录批处理、用户创建原子性和质量门禁仍有明确缺口。

## 高风险问题

### 管理凭证被持久化到浏览器 localStorage

`ADSS_MANAGEMENT_TOKEN` 是管理面生产密钥，但 `useAdminApi` 在加载、显式保存及成功鉴权后都会读取或写入 `localStorage`。[useAdminApi.ts](../../center/web/app/composables/useAdminApi.ts#L43) 将其恢复到页面状态，[useAdminApi.ts](../../center/web/app/composables/useAdminApi.ts#L53) 和 [useAdminApi.ts](../../center/web/app/composables/useAdminApi.ts#L97) 将其持久化。安全边界明确要求该 token 只能受限保存，且管理接口可写入目录、域配置和凭据事实。[security-boundary.md](../reference/security-boundary.md#L45)

同源脚本执行、浏览器配置文件读取或共用工作站都会取得长期管理凭证。管理 token 应默认仅保存在内存中并在页面关闭后失效；若确有持久会话需求，应改由服务端签发具备过期和撤销语义的受保护会话，不能复用部署级共享密钥。

### 生产密钥只校验非空

Center 允许任意非空 `ADSS_MANAGEMENT_TOKEN`、`ADSS_USER_SESSION_KEY` 和密码加密密钥启动：[state.rs](../../center/src/state.rs#L103)、[session.rs](../../center/src/session.rs#L42) 和 [password/mod.rs](../../center/src/password/mod.rs#L24)。部署与安全文档要求它们使用高熵生产密钥。[security.md](../guide/security.md#L9)

一字符或可猜测值会分别造成管理面完全控制、会话伪造和数据库中密码材料可被离线尝试。应对每类密钥定义明确的编码与最小随机字节数，并为生产配置拒绝弱值；开发测试使用显式构造器，不应放宽生产环境变量校验。

### Connector 真实模式允许明文 LDAP 承载绑定凭据和密码写入

真实 Connector 同时接受 `ldap://` 与 `ldaps://`。[config.rs](../../connector/src/config.rs#L71) 随后使用 simple bind 发送服务账号密码，[ldap.rs](../../connector/src/directory/ldap.rs#L37)；同一连接还通过 `unicodePwd` 下发用户明文密码。[ldap.rs](../../connector/src/directory/ldap.rs#L327)

即使网络被视为受保护，这条路径也会暴露 LDAP 服务账号凭据和密码同步材料，且 Active Directory 的密码修改通常要求受保护连接。真实写入模式应强制 LDAPS，或实施并强制 StartTLS；仅 dry-run 可以保留无 LDAP 配置的路径。

### Connector 安装未收紧 .env 的继承 ACL

安装脚本只向 `LocalService` 追加 `.env` 的读取权限，没有禁用继承或移除既有主体。[install-service.ps1](../../deploy/connector/install-service.ps1#L42) 在现有运行目录中直接使用 `.env`，[install-service.ps1](../../deploy/connector/install-service.ps1#L66) 仅执行 `/grant`。该文件包含 Connector key 和 LDAP bind password，而部署文档建议将运行目录放在 `Program Files` 下。[README.md](../../deploy/connector/README.md#L1)

继承到的 `Users` 或其他本地账户读取权限不会因追加 `LocalService` 权限而消失，导致域绑定密钥和 LDAP 服务账号可能被非服务身份读取。安装程序应对 `.env` 显式禁用继承，只保留 `SYSTEM`、`Administrators` 与 `LocalService` 的必要权限，并用脚本契约测试覆盖最终 ACL。

### Nuxt 构建带警告仍被视为成功

`bun run build` 返回成功，但 Nuxt/Nitro 输出 `cache-driver.mjs` 未解析而被 externalize、H3 named exports 未使用两条警告。前端依赖使用范围版本约束，[package.json](../../center/web/package.json#L12) 会使可解析的 Nuxt/Nitro/H3 组合随安装漂移。

当前构建不能作为零警告交付物放行。应定位这两条警告对应的依赖组合或构建配置，固定兼容版本并把零警告构建纳入门禁。

## 中风险问题

### 用户目录与初始凭据不是同一事务

管理端创建用户先持久化目录记录，随后在另一次写入中保存初始密码。[admin.rs](../../center/src/routes/admin.rs#L187) 这两步之间任一密码哈希、加密或存储失败都会返回错误，但目录事实已推进，留下没有可用凭据的活动用户。现有契约测试只覆盖全成功路径。[api_contract.rs](../../center/tests/api_contract.rs#L636)

应在 Repository 或服务层提供“创建目录用户并初始化凭据”的单一事务，或在后续失败时作受控补偿；同时覆盖第二步失败不遗留半创建状态的回归测试。

### 目录同步忽略 batch_limit，首次重建无上界

Center 将 `state.batch_limit` 传入目录查询。[routes.rs](../../center/src/routes.rs#L234)，但 Store 将参数命名为 `_limit` 且对三个集合均执行无上限查询，固定返回 `has_more: false`。[repository.rs](../../crates/store/src/repository.rs#L619)

大域首次同步或长期离线重建会生成无界响应，并让 Connector 在单次请求中串行执行全部 LDAP 操作。应按 revision 设计可确认的分页，或明确单次全量同步的容量上限并在服务端强制拒绝超过上限的请求。

### 前端缺少 lint、格式检查和测试入口

前端 `package.json` 仅定义开发、生成、构建和 typecheck 脚本。[package.json](../../center/web/package.json#L5) 仓库没有 ESLint/Prettier 配置；已有 `tests/oidc-ui.test.ts`，但没有稳定的测试脚本。参考文档也只列出 typecheck 与 build。[README.md](../reference/README.md#L64)

这会使 Vue/TypeScript 风格、静态错误和已有单元测试无法由一个可重复命令验证。应以 Bun 增加只检查的 lint、format check、test 入口并在文档中列出。

### 根验证命令遗漏被排除的 protocol 与 store crate

根 `Cargo.toml` 将 `crates/protocol` 与 `crates/store` 排除在 workspace 外。[Cargo.toml](../../Cargo.toml#L1) 但参考文档把 `cargo test --workspace` 和一次 root clippy 当作 Rust 修改后的完整验证命令。[README.md](../reference/README.md#L54)

这两个 crate 的改动不会被该入口覆盖，导致协议与持久化层的回归可在日常验证中遗漏。文档或统一检查脚本应显式串行执行两个 manifest 的 fmt check、test 和 clippy，与 workspace 验证共同构成完整门禁。

### 产品概览仍将已交付的 OIDC 排除在范围外

产品概览将 “OIDC、SAML、AD FS 等身份联邦” 列为不处理范围。[overview.md](../guide/overview.md#L87) 实际代码、README、部署和安全文档均已提供 OIDC Provider、客户端管理、签名私钥和授权码流程。

该漂移会使部署边界、风险评估和接入方预期错误。应将概览更新为“支持受限 OIDC Provider，SAML/AD FS 等其他联邦不在范围内”，并在文档变更时执行术语和边界一致性检查。

## 已执行检查

以下检查在本机完成：

- `cargo fmt --all -- --check`
- `cargo fmt --manifest-path crates/protocol/Cargo.toml -- --check`
- `cargo fmt --manifest-path crates/store/Cargo.toml -- --check`
- `bun test`：7 项测试通过
- `bun run typecheck`：通过
- `bun run build`：产物生成，但存在两条 Nuxt/Nitro 警告
- `pwsh -NoProfile -File scripts/test-docker-contract.ps1`：通过
- `pwsh -NoProfile -File scripts/test-connector-service-scripts.ps1`：通过
- `pwsh -NoProfile -File scripts/test-release-contract.ps1`：通过
- `git diff --check`：通过

未执行 Cargo clippy、Cargo test、Cargo build 和真实服务启动，避免在本次只读审查中写入或重建 `target`。未验证真实 AD/LDAPS、TLS 反向代理、浏览器 E2E、外部 OIDC 客户端兼容性和生产权限。
