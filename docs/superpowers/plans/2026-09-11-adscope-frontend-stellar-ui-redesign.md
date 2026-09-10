# Adscope 前端 Stellar UI 重构实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 使用 `@stellar/ui@0.3.0` 完成 Adscope Center 全部 Nuxt 页面的一致性重构。

**Architecture:** 通过 `@stellar/ui/nuxt` 提供基础组件、主题和全局反馈能力；保留现有 API composables、类型和业务组件职责，逐批替换页面表现层。

**Tech Stack:** Nuxt 4、Vue 3、Bun、Tailwind CSS 4、`@stellar/ui@0.3.0`、TypeScript。

**Spec:** `docs/superpowers/specs/2026-09-11-adscope-frontend-stellar-ui-redesign-design.md`

## Global Constraints

- 使用 Bun，不使用 npm、npx 或 package-lock。
- 固定依赖版本 `@stellar/ui@0.3.0`。
- 不改变 Center API、路由、数据模型和 AD 业务语义。
- 不在 `main.css` 直接导入 `@stellar/ui/styles`，由 Nuxt 模块注入。
- 每个批次完成后执行 `bun run typecheck`，最终执行 `bun run build`。

### Task 1: 接入 Stellar UI 和全局主题

**Files:**
- Modify: `center/web/package.json`
- Modify: `center/web/nuxt.config.ts`
- Modify: `center/web/app/assets/css/main.css`
- Modify: `center/web/app/app.vue`

- [ ] 使用 Bun 添加 `@stellar/ui@0.3.0`、Tailwind CSS 4 及 Nuxt 集成所需依赖。
- [ ] 配置 `@stellar/ui/nuxt`，保留应用 CSS 在组件库样式之后生效。
- [ ] 将全局颜色、页面背景、焦点和滚动条样式收敛到应用 CSS。
- [ ] 运行 `bun run typecheck`。

### Task 2: 重构管理端壳层和公共组件

**Files:**
- Modify: `center/web/app/components/admin/AdminShell.vue`
- Modify: `center/web/app/components/admin/PageHeader.vue`
- Modify: `center/web/app/components/admin/Modal.vue`
- Modify: `center/web/app/components/admin/StatusBadge.vue`
- Modify: `center/web/app/components/admin/StatusLine.vue`
- Modify: `center/web/app/components/admin/EmptyState.vue`
- Modify: `center/web/app/components/admin/PaginationBar.vue`

- [ ] 使用 Stellar Button、Input、Badge、Modal、Toast、Confirm 和 Pagination 替换原生基础控件。
- [ ] 统一导航、凭证登录、刷新、退出、页面标题和状态反馈。
- [ ] 保留当前管理 API 和忙碌状态阻止导航行为。
- [ ] 运行 `bun run typecheck`。

### Task 3: 重构用户端和授权页面

**Files:**
- Modify: `center/web/app/pages/login.vue`
- Modify: `center/web/app/pages/me.vue`
- Modify: `center/web/app/pages/authorize.vue`
- Modify: `center/web/app/pages/index.vue`

- [ ] 统一登录、资料、密码和 OAuth 授权页面的面板、表单和反馈。
- [ ] 保留现有 user/OIDC API 和字段。
- [ ] 运行 `bun run typecheck`。

### Task 4: 重构目录和域管理

**Files:**
- Modify: `center/web/app/pages/admin/index.vue`
- Modify: `center/web/app/pages/admin/domains.vue`
- Modify: `center/web/app/components/directory/*.vue`
- Modify: `center/web/app/components/domains/*.vue`

- [ ] 统一树、表格、筛选、创建、编辑、禁用和删除操作。
- [ ] 对 OU、用户和组保留现有业务模型及目录关系。
- [ ] 对危险操作接入 Confirm，并对结果接入 Toast。
- [ ] 运行 `bun run typecheck`。

### Task 5: 重构 OAuth Client 和同步状态

**Files:**
- Modify: `center/web/app/pages/admin/clients.vue`
- Modify: `center/web/app/pages/admin/sync.vue`
- Modify: `center/web/app/components/oauth/*.vue`
- Modify: `center/web/app/components/sync/SyncTable.vue`

- [ ] 统一 Client 编辑、密钥展示、回调地址、同步状态和错误呈现。
- [ ] 区分目录 revision、凭据 revision、成功、失败和未同步状态。
- [ ] 运行 `bun run typecheck`。

### Task 6: 收口验证

**Files:**
- Modify: `center/web/app/assets/css/main.css`
- Modify: `center/web/tests/oidc-ui.test.ts` when existing assertions require the new structure

- [ ] 删除已无调用方的旧基础控件样式。
- [ ] 运行 `bun run typecheck`。
- [ ] 运行 `bun run build`。
- [ ] 使用 `git diff --check` 检查全部差异。
