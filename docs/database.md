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

## 边界

当前数据库层已经接入主服务启动和用户更新路径，能持久化 desired state 快照与审计事件。密码任务、Agent cursor、注册令牌、drift report 仍保留在内存 store 中，后续应逐步拆成关系表。

密码材料不得直接落明文列。后续增加密码任务表时，只能保存 KMS/HSM 返回的密文引用或密文材料，并保持审计与日志不可见。
