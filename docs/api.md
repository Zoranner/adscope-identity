# API 契约

## 总体边界

MVP 主服务只暴露中心改密、用户目录字段更新和 Agent 主动同步接口。中心数据库是唯一事实源，域内 AD 不向中心反向写入。

所有 Agent 接口必须携带请求头 `x-adss-agent-key`。服务端根据 `domain_id` 读取 `domains.agent_key_hash`，对请求 key 做摘要后校验；未知域、错误 key 或缺失 key 返回 `401 Unauthorized`，已认证但域被禁用返回 `403 Forbidden`。

## 用户登录

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
- 不创建 session 或 token；当前只固定中心改密前的身份校验边界。
- 验证失败返回 `401 Unauthorized`。

响应：

```json
{
  "employee_id": "1001"
}
```

## 更新用户目录字段

`PATCH /api/users/{employee_id}`

请求字段对应 `UserDirectoryPatch`，用于更新中心当前目录事实：

```json
{
  "username": "zhangsan",
  "display_name": "张三",
  "email": "zhangsan@example.com",
  "mobile": "13800000000",
  "telephone": "021-10000000",
  "organizational_unit_id": "ou-rd",
  "status": "active"
}
```

行为：

- 一次请求在一个目录 revision 中写入该用户当前状态。
- `status` 仅支持 `active` 和 `disabled`。
- 任意用户目录字段变化都会推进 `sync_metadata.directory_revision`。

响应：

```json
{
  "employee_id": "1001",
  "directory_revision": 12
}
```

## 修改用户密码

`POST /api/users/{employee_id}/password`

请求：

```json
{
  "current_password": "CurrentPass123!",
  "new_password": "NewPass123!"
}
```

行为：

- 只能从中心服务修改密码。
- 先用当前 `password_verifier` 校验旧密码。
- 成功后写入新的 `password_verifier` 和 `password_ciphertext`，并推进 `sync_metadata.credential_revision`。
- 响应不返回密码明文或密文。

响应：

```json
{
  "employee_id": "1001",
  "credential_revision": 8
}
```

## Agent 同步

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

行为：

- 目录和凭据是两个独立通道。
- `rebuild_directory=true` 时目录通道从 revision `0` 重建。
- `rebuild_credentials=true` 时凭据通道从 revision `0` 重建。
- 目录响应只包含 `changed_revision` 大于请求进度的当前对象。
- 凭据响应由服务端在内存中解封密文，返回 Agent 可直接设置到 AD 的明文密码。
- 成功响应设置 `Cache-Control: no-store`。

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
    "employee_id_attribute": "employeeID"
  }
}
```

## Agent 确认

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
- 只有 `success=true` 时才推进对应通道的 applied revision。
- `success=false` 记录为失败确认语义，当前不推进 revision。
- 服务端拒绝倒退确认，也拒绝超过当前全局 revision 的确认。

响应：

```json
{
  "accepted": true
}
```
