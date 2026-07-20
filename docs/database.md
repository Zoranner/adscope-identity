# 数据库设计

## 当前事实源

系统使用 SeaORM 封装数据库访问，主服务通过 `Repository` 读写当前事实。中心数据库是运行时唯一事实源，不再使用内存业务 Store 承担主链同步状态。

当前 schema 仍通过 `CREATE TABLE IF NOT EXISTS` 初始化。正式生产迁移工具、schema 版本和历史归档属于后续任务。

## 当前表

`sync_metadata`

- `key`：元数据主键，当前固定为默认行。
- `directory_revision`：目录通道当前全局 revision。
- `credential_revision`：凭据通道当前全局 revision。

`organizational_units`

- `id`：中心稳定 OU 标识。
- `name`：OU 名称。
- `parent_id`：父 OU 标识，根节点为空。
- `changed_revision`：该 OU 当前状态最后一次变化所在的目录 revision。

`users`

- `employee_id`：跨域唯一用户标识。
- `username`：AD 账号名，也作为 UPN 本地部分来源。
- `display_name`：显示名。
- `email`：邮箱。
- `mobile`：手机号。
- `telephone`：办公电话。
- `organizational_unit_id`：用户目标 OU。
- `status`：`active` 或 `disabled`。
- `changed_revision`：该用户当前目录状态最后一次变化所在的目录 revision。

`groups`

- `id`：中心稳定组标识。
- `name`：组名，映射为 AD 组 CN 和账号名。
- `member_employee_ids_json`：成员工号数组 JSON。
- `changed_revision`：该组当前状态最后一次变化所在的目录 revision。

`user_credentials`

- `employee_id`：用户标识。
- `password_ciphertext`：中心保存的当前密码密文。
- `password_verifier`：中心登录和改密校验使用的 verifier。
- `changed_revision`：该用户当前凭据最后一次变化所在的凭据 revision。

`domains`

- `id`：域标识，也是 Agent 同步请求中的 `domain_id`。
- `name`：域显示名。
- `enabled`：域是否允许同步。
- `mirror_root_dn`：本系统管理对象所在镜像根 DN。
- `quarantine_ou_dn`：禁用用户隔离 OU DN。
- `upn_suffix`：该域 UPN 后缀。
- `employee_id_attribute`：AD 中保存工号的属性名。
- `agent_key_hash`：Agent key 摘要，不保存明文 key。
- `applied_directory_revision`：该域已确认应用的目录 revision。
- `applied_credential_revision`：该域已确认应用的凭据 revision。

## Revision 规则

一次中心目录写事务分配一个新的 `directory_revision`，本次涉及的 OU、用户和组共享该 revision。Agent 拉取时按对象 `changed_revision > applied_directory_revision` 返回当前完整对象状态。

一次中心改密事务分配一个新的 `credential_revision`，只保留该用户最新的 `user_credentials` 行。Agent 拉取时按 `changed_revision > applied_credential_revision` 返回待设置的当前密码材料。

确认写入只更新 `domains` 中目标通道的一列，并拒绝倒退或超过全局 revision 的确认。
