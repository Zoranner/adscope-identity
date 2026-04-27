# 数据库设计

## ORM 选择

数据库层使用 SeaORM。当前代码将 ORM 封装在 `adss-persistence` crate 中，主服务只依赖 repository 能力，不直接操作具体表。

当前私有 Cargo 源尚未提供 SeaORM `2.0.0` stable，项目锁定 `2.0.0-rc.38`。后续私有源同步 stable 后，应单独升级并跑完整验证。

## 当前表

`state_documents`

- `key`：状态文档主键。
- `value_json`：序列化后的主服务状态快照。

`audit_events`

- `sequence`：审计序号。
- `actor`：操作者或 Agent。
- `action`：动作名。
- `target`：目标对象。
- `result`：结果。
- `detail_json`：非敏感摘要 JSON。

`password_tasks`

- `task_id`：密码任务序号。
- `domain_id`：目标域。
- `employee_id`：目标用户工号。
- `encrypted_password`：密码密文或密文引用，禁止明文。

`agent_cursors`

- `agent_id`：Agent 标识。
- `domain_id`：绑定域。
- `structure_version`：已应用结构版本。
- `password_task_cursor`：已处理密码任务游标。

`drift_reports`

- `id`：drift report 序号。
- `domain_id`：上报域。
- `agent_id`：上报 Agent。
- `observed_structure_version`：Agent 观察到的结构版本。
- `drifted_objects_json`：drift 对象摘要。

`registration_tokens`

- `token`：一次性注册令牌。
- `domain_id`：令牌允许绑定的域。

## 边界

当前数据库层已经接入主服务启动、用户更新、密码任务创建、Agent report、drift report 和 Agent 注册令牌消费路径。主服务仍保留内存 store 作为运行时聚合缓存，但关键同步状态已经有 ORM 持久化表承接。

密码材料不得直接落明文列。`password_tasks.encrypted_password` 只能保存 KMS/HSM 返回的密文引用或密文材料，并保持审计与日志不可见。
