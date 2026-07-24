# API 契约

## 设计原则

API 按调用身份分组：

- `/api/auth/*` 用于普通用户登录。
- `/api/me/*` 用于普通用户自助，只能操作 token 对应的本人账号。
- `/api/admin/*` 用于受保护管理入口，写入中心目录和域配置事实。
- `/api/agent/*` 用于域内 Agent 同步，必须通过域绑定的 Agent key 鉴权。

中心数据库是 API 写入的唯一事实源。普通用户、管理员和 Agent 都不直接写 AD；AD 写入只由域内 Agent 通过同步协议执行。

管理入口必须和普通用户自助入口区分身份边界。本文档不定义管理员账号模型、角色模型或操作记录平台；这些只有在有明确组织权限和合规要求时才单独建模。

## 通用约定

请求和响应使用 JSON。服务端错误使用标准 HTTP 状态码：

| 状态码 | 含义 |
| --- | --- |
| `400 Bad Request` | 请求结构或字段值非法。 |
| `401 Unauthorized` | 缺少凭证、凭证错误或登录失败。 |
| `403 Forbidden` | 已认证但无权访问目标资源。 |
| `404 Not Found` | 目标对象不存在。 |
| `409 Conflict` | 请求与唯一约束、revision 或状态规则冲突。 |
| `500 Internal Server Error` | 服务端持久化、密码加密或外部依赖错误。 |

密码明文、密码密文和 Agent key 明文不得出现在普通查询响应、错误响应或日志中。Agent key 生成或替换时只在本次响应返回明文。

## 普通用户接口

### 用户登录

`POST /api/auth/login`

请求：

```json
{
  "employee_id": "1001",
  "password": "CurrentPass123!"
}
```

行为：

- 使用 `user_credentials.password_verifier` 验证用户提交的密码。
- 登录成功后签发普通用户自助接口使用的 Bearer token。
- 登录失败返回 `401 Unauthorized`。

响应：

```json
{
  "employee_id": "1001",
  "access_token": "adss-user-session:v1:1001:..."
}
```

### 本人资料

以下接口必须携带普通用户 token：

```text
Authorization: Bearer <access_token>
```

`GET /api/me`

行为：

- 根据 token 读取当前用户自己的目录资料。
- 不接受 employee_id、username 或其他用户选择参数。
- 缺少 token、token 过期或签名错误时返回 `401 Unauthorized`。

响应：

```json
{
  "employee_id": "1001",
  "username": "zhangsan",
  "display_name": "张三",
  "email": "zhangsan@example.com",
  "mobile": "13800000000",
  "telephone": "021-10000000",
  "organizational_unit_id": "ou-rd",
  "status": "active"
}
```

### 本人联系方式

`PATCH /api/me/contact`

请求：

```json
{
  "email": "zhangsan@example.com",
  "mobile": "13800000000",
  "telephone": "021-10000000"
}
```

行为：

- 只允许修改 token 对应用户的 `email`、`mobile` 和 `telephone`。
- 不接受 `employee_id`、`username`、`display_name`、`organizational_unit_id`、`status` 或组成员字段。
- 成功后推进目录 revision，由 Agent 同步到各域。

响应：

```json
{
  "profile": {
    "employee_id": "1001",
    "username": "zhangsan",
    "display_name": "张三",
    "email": "zhangsan@example.com",
    "mobile": "13800000000",
    "telephone": "021-10000000",
    "organizational_unit_id": "ou-rd",
    "status": "active"
  },
  "directory_revision": 12
}
```

### 本人改密

`POST /api/me/password`

请求：

```json
{
  "current_password": "CurrentPass123!",
  "new_password": "NewPass123!"
}
```

行为：

- 根据 token 确定当前用户。
- 先校验当前密码，再写入新的 verifier 和 ciphertext。
- 成功后推进凭据 revision。

响应：

```json
{
  "employee_id": "1001",
  "credential_revision": 8
}
```

## 管理入口

管理入口必须携带受保护管理凭证。凭证格式由接入层决定，不能复用普通用户自助 token。

```text
Authorization: Bearer <management_token>
```

管理写入只维护中心当前事实，不直接访问域控。

### 域管理

`GET /api/admin/domains` 查询域列表、启用状态和已确认 revision。

`POST /api/admin/domains` 创建域配置：

```json
{
  "id": "domain-a",
  "name": "A 域",
  "enabled": true,
  "mirror_root_dn": "OU=Mirror,DC=a,DC=example,DC=com",
  "quarantine_ou_dn": "OU=Quarantine,DC=a,DC=example,DC=com",
  "upn_suffix": "a.example.com",
  "employee_id_attribute": "employeeID",
  "managed_group_id_attribute": "adminDescription",
  "agent_key": "generated-or-imported-agent-key"
}
```

`PATCH /api/admin/domains/{domain_id}` 更新域名称、启用状态、镜像根、隔离 OU、UPN 后缀、工号属性和受管组标识属性。`agent_key_hash` 不能通过普通 PATCH 更新。

`POST /api/admin/domains/{domain_id}/agent-key` 替换 Agent key，并只在本次响应返回明文 key：

```json
{
  "domain_id": "domain-a",
  "agent_key": "new-agent-key"
}
```

### OU 管理

`GET /api/admin/ous/tree` 查询中心 OU 树。

`POST /api/admin/ous` 创建 OU：

```json
{
  "id": "ou-rd",
  "name": "研发部",
  "parent_id": null
}
```

`PATCH /api/admin/ous/{ou_id}` 更新 OU 名称或父级。物理删除 OU 需要先定义用户、子 OU 和组引用处理规则。

### 用户管理

`GET /api/admin/users` 查询用户列表，支持按 `employee_id`、`username`、`organizational_unit_id` 和 `status` 过滤。

`POST /api/admin/users` 创建用户目录事实并初始化凭据：

```json
{
  "employee_id": "1001",
  "username": "zhangsan",
  "display_name": "张三",
  "email": "zhangsan@example.com",
  "mobile": "13800000000",
  "telephone": "021-10000000",
  "organizational_unit_id": "ou-rd",
  "status": "active",
  "initial_password": "InitialPass123!"
}
```

`GET /api/admin/users/{employee_id}` 查询用户详情。

`PATCH /api/admin/users/{employee_id}` 更新用户目录字段。

`POST /api/admin/users/{employee_id}/disable` 禁用用户。

`POST /api/admin/users/{employee_id}/enable` 启用用户。

`POST /api/admin/users/{employee_id}/password-reset` 管理员重置或代设密码：

```json
{
  "new_password": "ResetPass123!"
}
```

管理员重置密码不要求用户当前密码，必须保留可追溯记录。

### 组管理

`GET /api/admin/groups` 查询组列表。

`POST /api/admin/groups` 创建组：

```json
{
  "id": "group-rd",
  "name": "研发部"
}
```

`GET /api/admin/groups/{group_id}` 查询组详情和成员。

`PATCH /api/admin/groups/{group_id}` 更新组名。

`PUT /api/admin/groups/{group_id}/members` 用完整集合替换组成员：

```json
{
  "member_employee_ids": [
    "1001",
    "1002"
  ]
}
```

组成员集合是事实源，不单独暴露成员增删事件接口。

### 同步状态

`GET /api/admin/sync/domains` 查询各域同步状态。

响应只使用现有域进度和全局 revision 推导：

```json
{
  "domains": [
    {
      "domain_id": "domain-a",
      "enabled": true,
      "applied_directory_revision": 12,
      "applied_credential_revision": 8,
      "directory_lag": 0,
      "credential_lag": 0
    }
  ]
}
```

rebuild 由 Agent 请求中的 `rebuild_directory` 和 `rebuild_credentials` 标志触发。

## Agent 接口

所有 Agent 接口必须携带：

```text
x-adss-agent-key: <agent-key>
```

服务端按请求 `domain_id` 校验 `domains.agent_key_hash`。未知域、错误 key 或缺少 key 返回 `401 Unauthorized`，域被禁用返回 `403 Forbidden`。

### Agent 同步

`POST /api/agent/sync`

请求：

```json
{
  "domain_id": "domain-a",
  "applied_directory_revision": 10,
  "applied_credential_revision": 7,
  "rebuild_directory": false,
  "rebuild_credentials": false
}
```

响应：

```json
{
  "directory": {
    "server_revision": 12,
    "batch_revision": 12,
    "organizational_units": [],
    "users": [],
    "groups": [],
    "has_more": false
  },
  "credentials": {
    "server_revision": 8,
    "batch_revision": 8,
    "credentials": [],
    "has_more": false
  },
  "directory_config": {
    "domain_id": "domain-a",
    "mirror_root_dn": "OU=Mirror,DC=a,DC=example,DC=com",
    "quarantine_ou_dn": "OU=Quarantine,DC=a,DC=example,DC=com",
    "upn_suffix": "a.example.com",
    "employee_id_attribute": "employeeID",
    "managed_group_id_attribute": "adminDescription"
  }
}
```

凭据响应包含 Agent 可执行的明文密码。Agent 调用主服务 `/api/agent/sync` 时必须走 TLS，并设置 `Cache-Control: no-store`。

### Agent 确认

`POST /api/agent/confirm`

请求：

```json
{
  "domain_id": "domain-a",
  "channel": "directory",
  "target_revision": 12,
  "success": true,
  "error_code": null
}
```

行为：

- `channel` 只能是 `directory` 或 `credential`。
- `success=true` 时推进对应通道的 applied revision。
- `success=false` 时接受失败回报，但不推进 revision。
- 服务端拒绝倒退确认。
- 服务端拒绝超过当前全局 revision 的确认。

响应：

```json
{
  "accepted": true
}
```
