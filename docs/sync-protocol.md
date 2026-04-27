# Agent 同步协议

## 轮询

Agent 使用 `POST /api/agent/poll` 主动轮询主服务。请求包含：

- `domain_id`：Agent 所属域。
- `agent_id`：Agent 身份。
- `last_structure_version`：本地已成功应用的结构版本。
- `password_task_cursor`：本地已成功处理的密码任务游标。

主服务必须验证 Agent 与域的绑定关系。绑定 A 域的 Agent 不能拉取 B 域任务。

## 注册

Agent 使用 `POST /api/agent/register` 完成首次绑定。请求包含：

- `registration_token`：主服务预先签发的一次性注册令牌。
- `agent_id`：Agent 实例标识。
- `domain_id`：Agent 所属域。
- `certificate_subject`：客户端证书主体，用于后续 mTLS 绑定。

注册令牌只能使用一次，并且只能绑定令牌所属域。注册成功后，Agent 才允许轮询该域的同步任务。

## 结构载荷

响应中的 `structure` 有三种形态：

- `no_change`：结构版本一致，无需 reconcile。
- `delta`：返回从上次版本到当前版本的目标状态变化。
- `snapshot`：返回完整目标状态，用于首次同步、cursor 过旧或状态不可信时重建。

首版实现可以把 `delta` 表示为当前目标状态，由 Agent 端幂等 reconcile。后续当数据规模增大时再细化对象级差异。

## 密码任务

响应中的 `password_tasks` 只包含当前域且 `task_id > password_task_cursor` 的任务。任务对象不得序列化明文密码。真实实现中 Agent 应在受控路径中解密或获取一次性下发材料，并立即调用 LDAPS 设置密码。

## 结果回传

Agent 使用 `POST /api/agent/report` 回传：

- 已应用结构版本。
- 已处理密码任务 cursor。
- 成功、失败、跳过、待人工处理统计。
- 对象级结果、错误码和 AD 拒绝原因。

主服务只在认证通过且 report 合法时推进 cursor。失败任务保留可重试状态，禁止静默丢弃。

## Drift 回传

Agent 使用 `POST /api/agent/drift-report` 回传本地域摘要对账结果。Drift 只进入告警和审计，不触发 AD 到主服务的反向写入。

域状态通过 `GET /api/sync/domains/{domain_id}/status` 查询。状态至少包含目标结构版本、已应用结构版本、密码任务游标和 drift 数量。
