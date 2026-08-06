# 数据库设计

## 设计原则

中心数据库是账号和目录同步的唯一事实源。主服务通过 `Repository` 读写数据库，Connector 只通过 HTTP API 获取期望状态，不直接访问数据库。

数据库保存当前事实，不保存历史事件流：

- 目录事实只保存 OU、用户和组的当前状态。
- 凭据事实只保存用户当前 verifier 和 ciphertext。
- 域记录保存该域已确认的目录和凭据 revision。
- OAuth 客户端表保存预登记客户端的当前配置；授权码表只保存等待兑换的短期一次性记录。
- 对象多次变更后，Connector 只拉取最新完整状态。

浏览器 SSO 登录状态保存在签名 Cookie 中，ID Token 和 access token 是无状态 JWT。数据库不增加浏览器会话、历史同意、授权历史或 token 表。

数据库 schema 由主服务初始化逻辑创建基础表。

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
| `username` | AD 账号名，也作为 UPN 本地部分来源。该字段唯一，并作为普通用户登录名。 |
| `display_name` | 显示名。 |
| `email` | 邮箱，允许普通用户自助修改。 |
| `mobile` | 手机号，允许普通用户自助修改。 |
| `telephone` | 办公电话，允许普通用户自助修改。 |
| `organizational_unit_id` | 用户目标 OU。 |
| `status` | 用户状态，取值为 `active` 或 `disabled`。 |
| `changed_revision` | 该用户目录状态最后一次变化所在的目录 revision。 |

### `groups`

保存组事实、目标 OU 和成员集合。

| 字段 | 含义 |
| --- | --- |
| `id` | 中心稳定组标识。 |
| `name` | 组名，映射为 AD 组 CN 和账号名。 |
| `organizational_unit_id` | 组目标 OU。 |
| `member_employee_ids` | 成员工号数组 JSON。 |
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

`password_verifier` 不能还原密码。`password_ciphertext` 只能由主服务通过密码加密方式解封，并只在响应 Connector 凭据同步时短暂进入内存。

### `oauth_clients`

保存预登记 OIDC 客户端的当前配置。

| 字段 | 含义 |
| --- | --- |
| `client_id` | Center 生成的客户端主键，创建后不可修改。 |
| `name` | 管理端和授权确认页使用的显示名称。 |
| `client_type` | 客户端类型，存储值为 `web` 或 `desktop`。 |
| `client_secret_hash` | Web 客户端 secret 的 `sha256:` 摘要；Desktop 客户端为空。 |
| `redirect_uris` | 登记回调地址数组，以 JSON 文本保存。 |
| `allowed_scopes` | 客户端可申请的 scope 数组，以 JSON 文本保存。 |
| `enabled` | 客户端能否发起授权、兑换授权码和调用 UserInfo。 |

Web 客户端创建或重新生成 secret 时，明文只通过带 `Cache-Control: no-store` 的管理响应返回。数据库只保存新 secret 的摘要，重新生成时直接替换旧摘要，不保存 secret 版本或并行有效期。Desktop 客户端不生成 secret。

删除客户端会移除该客户端配置。授权码兑换时仍会读取客户端记录；客户端不存在或已停用时，授权码不能换取令牌。

### `oauth_authorization_codes`

保存用户确认后生成、等待 Token 端点兑换的一次性授权码记录。

| 字段 | 含义 |
| --- | --- |
| `code_hash` | 授权码的 `sha256:` 摘要，也是表主键。 |
| `client_id` | 授权码绑定的客户端 ID。 |
| `employee_id` | 授权用户的稳定标识，对应 OIDC `sub`。 |
| `redirect_uri` | 授权请求实际使用的回调地址，Token 请求必须完全一致。 |
| `scopes` | 用户本次确认的 scope 数组，以 JSON 文本保存。 |
| `nonce` | 授权请求中的 nonce，用于签发 ID Token。 |
| `code_challenge` | 授权请求中的 S256 PKCE challenge。 |
| `auth_time` | 浏览器登录发生时间的 Unix 秒数。 |
| `expires_at` | 授权码到期时间的 Unix 秒数。 |

授权码明文只通过已验证的 `redirect_uri` 返回，不写入数据库。记录有效期为 120 秒；Token 端点按 `code_hash` 原子删除记录后再完成兑换校验，所以同一授权码最多只有一个并发请求能够取得记录。未知、过期、已兑换或绑定信息不匹配的授权码都不能产生令牌。

授权码没有已使用状态字段。兑换会删除目标记录；生成新授权码前会按到期时间有限批量清理过期记录，不保存兑换结果、历史同意或授权事件。

### `domains`

保存域配置和同步进度。

| 字段 | 含义 |
| --- | --- |
| `id` | 域标识，也是 Connector 同步请求中的 `domain_id`。 |
| `name` | 域显示名。 |
| `enabled` | 域是否允许同步。 |
| `mirror_root_dn` | 本系统管理对象所在镜像根 DN。 |
| `quarantine_ou_dn` | 禁用用户隔离 OU DN。 |
| `upn_suffix` | 该域 UPN 后缀。 |
| `employee_id_attribute` | AD 中保存工号的属性名。 |
| `managed_group_id_attribute` | AD 受管组对象中保存中心组标识的属性名，默认 `adminDescription`。 |
| `connector_key_hash` | Connector key 摘要，不保存明文 key。 |
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

OAuth 客户端写入不推进目录或凭据 revision。创建客户端时生成稳定 `client_id`，客户端类型和 ID 在普通更新中保持不变；Web secret 只能通过创建或 secret 重新生成接口写入摘要。

授权确认只写入一条短期 `oauth_authorization_codes` 记录。授权码兑换在数据库事务内删除目标记录，并在同一事务中读取绑定的客户端和用户；数据库不写入 token、会话或授权历史。

Connector 确认只更新 `domains` 中目标通道的 applied revision。服务端必须拒绝倒退确认，也必须拒绝超过全局 revision 的确认。

## 查询规则

目录同步查询按 `changed_revision > applied_directory_revision` 返回当前对象。凭据同步查询按 `changed_revision > applied_credential_revision` 返回当前凭据材料。

查询返回的是完整当前对象，不返回历史差异。分页或批次切分不能越过本批实际返回对象的最大 `changed_revision`。
