# 数据库设计

## 设计原则

中心数据库是账号和目录同步的唯一事实源。主服务通过 `Repository` 读写数据库，Agent 只通过 HTTP API 获取期望状态，不直接访问数据库。

数据库保存当前事实，不保存历史事件流：

- 目录事实只保存 OU、用户和组的当前状态。
- 凭据事实只保存用户当前 verifier 和 ciphertext。
- 域记录保存该域已确认的目录和凭据 revision。
- 对象多次变更后，Agent 只拉取最新完整状态。

生产环境应使用受控迁移工具管理 schema 版本。开发和测试环境可以使用初始化逻辑创建基础表。

## 表结构

### `sync_metadata`

保存全局 revision。

| 字段 | 含义 |
| --- | --- |
| `key` | 元数据主键，默认行用于主链 revision。 |
| `directory_revision` | 目录通道全局 revision。 |
| `credential_revision` | 凭据通道全局 revision。 |

### `organizational_units`

保存中心 OU 树。

| 字段 | 含义 |
| --- | --- |
| `id` | 中心稳定 OU 标识。 |
| `name` | OU 名称。 |
| `parent_id` | 父 OU 标识，根节点为空。 |
| `changed_revision` | 该 OU 当前状态最后一次变化所在的目录 revision。 |

### `users`

保存用户目录事实。

| 字段 | 含义 |
| --- | --- |
| `employee_id` | 跨域唯一用户标识。 |
| `username` | AD 账号名，也作为 UPN 本地部分来源。 |
| `display_name` | 显示名。 |
| `email` | 邮箱，允许普通用户自助修改。 |
| `mobile` | 手机号，允许普通用户自助修改。 |
| `telephone` | 办公电话，允许普通用户自助修改。 |
| `organizational_unit_id` | 用户目标 OU。 |
| `status` | 用户状态，取值为 `active` 或 `disabled`。 |
| `changed_revision` | 该用户目录状态最后一次变化所在的目录 revision。 |

### `groups`

保存组事实和成员集合。

| 字段 | 含义 |
| --- | --- |
| `id` | 中心稳定组标识。 |
| `name` | 组名，映射为 AD 组 CN 和账号名。 |
| `member_employee_ids_json` | 成员工号数组 JSON。 |
| `changed_revision` | 该组当前状态最后一次变化所在的目录 revision。 |

组成员集合直接保存在组记录中。除非需要独立查询或独立一致性边界，否则不增加单独成员表。

### `user_credentials`

保存用户当前凭据事实。

| 字段 | 含义 |
| --- | --- |
| `employee_id` | 用户标识。 |
| `password_ciphertext` | 中心保存的当前密码密文。 |
| `password_verifier` | 中心登录和改密校验使用的 verifier。 |
| `changed_revision` | 该用户当前凭据最后一次变化所在的凭据 revision。 |

`password_verifier` 不能还原密码。`password_ciphertext` 只能由主服务通过密码加密方式解封，并只在响应 Agent 凭据同步时短暂进入内存。

### `domains`

保存域配置和同步进度。

| 字段 | 含义 |
| --- | --- |
| `id` | 域标识，也是 Agent 同步请求中的 `domain_id`。 |
| `name` | 域显示名。 |
| `enabled` | 域是否允许同步。 |
| `mirror_root_dn` | 本系统管理对象所在镜像根 DN。 |
| `quarantine_ou_dn` | 禁用用户隔离 OU DN。 |
| `upn_suffix` | 该域 UPN 后缀。 |
| `employee_id_attribute` | AD 中保存工号的属性名。 |
| `agent_key_hash` | Agent key 摘要，不保存明文 key。 |
| `applied_directory_revision` | 该域已确认应用的目录 revision。 |
| `applied_credential_revision` | 该域已确认应用的凭据 revision。 |

## 写入规则

目录写入必须在一个事务中完成：

- 分配新的 `directory_revision`。
- 写入受影响的 OU、用户和组。
- 将这些对象的 `changed_revision` 设置为同一个 revision。

普通用户联系方式更新只修改 `email`、`mobile` 和 `telephone`，并推进目录 revision。管理员修改用户目录字段、OU 或组成员也推进目录 revision。

凭据写入必须在一个事务中完成：

- 分配新的 `credential_revision`。
- 写入新的 `password_verifier` 和 `password_ciphertext`。
- 将该用户凭据的 `changed_revision` 设置为该 revision。

Agent 确认只更新 `domains` 中目标通道的 applied revision。服务端必须拒绝倒退确认，也必须拒绝超过全局 revision 的确认。

## 查询规则

目录同步查询按 `changed_revision > applied_directory_revision` 返回当前对象。凭据同步查询按 `changed_revision > applied_credential_revision` 返回当前凭据材料。

查询返回的是完整当前对象，不返回历史差异。分页或批次切分不能越过本批实际返回对象的最大 `changed_revision`。
