# API 契约

## 主服务接口

`PATCH /api/users/{employee_id}` 用于修改用户白名单属性。当前契约允许修改显示名、目标相对 DN 和属性集合；服务端必须过滤 AD 系统字段，禁止写入 `objectGUID`、`objectSid`、`distinguishedName` 等属性。

`POST /api/users/{employee_id}/password` 用于提交新密码。服务端为每个已接入域创建独立密码任务，响应只返回创建数量，不返回明文密码或可逆密文。

`PUT /api/org-tree` 用于提交新的目标组织树。服务端生成新的结构版本，Agent 后续通过轮询获取增量或快照。

`GET /api/sync/domains/{domain_id}/status` 用于查询域同步状态，包括目标结构版本、已应用结构版本、密码任务游标和 drift 数量。

`GET /api/audit/events` 用于查询审计事件。审计事件记录 actor、action、target、result 和非敏感 detail；密码明文、可逆密文和域控凭据不得出现在响应中。

## Agent 接口

`POST /api/agent/register` 使用一次性注册令牌绑定 Agent 与域。令牌使用后作废，响应返回 `agent_key`，该密钥只应在注册交付链路中保存一次。

`POST /api/agent/poll` 由 Agent 主动轮询结构状态和密码任务。请求必须携带 `X-ADSS-Agent-Key`，主服务必须校验 Agent 与 `domain_id` 的绑定关系以及共享密钥。

`POST /api/agent/report` 回传执行结果，并推进已应用结构版本和密码任务游标。请求必须携带 `X-ADSS-Agent-Key`。

`POST /api/agent/drift-report` 回传本地 AD 对账差异。请求必须携带 `X-ADSS-Agent-Key`。该接口只记录 drift，不允许触发反向写入。
