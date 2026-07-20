# Agent 同步协议

## 同步边界

每个域部署一个 Agent。Agent 定时主动请求主服务，主服务不主动连接域控或 Agent。中心数据库保存当前事实；域内 AD 是下游镜像，域内人工修改不传播回中心。

Agent 请求使用 `x-adss-agent-key` 认证，并且只能访问请求 `domain_id` 对应的域配置和同步数据。

## 双通道

MVP 同步拆成两个独立通道：

- `directory`：OU、用户资料、用户状态、用户目标 OU、组和组成员。
- `credential`：中心当前密码材料。

两个通道分别拉取、执行和确认。目录失败不阻塞凭据执行，凭据失败不回退目录确认。

## Sync 请求

`POST /api/agent/sync`

```json
{
  "domain_id": "domain-a",
  "applied_directory_revision": 10,
  "applied_credential_revision": 7,
  "rebuild_directory": false,
  "rebuild_credentials": false
}
```

`applied_*_revision` 表示 Agent 本地已经成功执行并被中心接受的 revision。请求 revision 不能高于中心为该域记录的 applied revision；如果上次 confirm 响应丢失，Agent 必须先重试 confirm。

`rebuild_directory=true` 表示目录通道从 revision `0` 重新拉取当前对象。`rebuild_credentials=true` 表示凭据通道从 revision `0` 重新拉取当前凭据。Agent 本地 state 文件无法解析时使用两个 rebuild 标记恢复。

## Sync 响应

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
    "server_revision": 9,
    "batch_revision": 9,
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

`server_revision` 是中心当前全局 revision。`batch_revision` 是本次响应允许 Agent 确认的最高 revision。Agent 只能确认 `batch_revision`，不能直接确认未返回的更高版本。

目录通道当前取消分页，`batch_revision` 等于 `server_revision`，`has_more=false`。凭据通道可按批返回，服务端保证 `batch_revision` 不越过本批实际返回范围。

## 目录执行

目录响应包含 changed revision 大于请求进度的当前对象状态。Agent 不重放旧中间状态。

Agent 将目录批次转为固定执行顺序：

- 父 OU 先于子 OU。
- 确保用户存在并同步属性。
- active 用户移动到目标 OU。
- 确保组存在。
- 同步组成员集合。
- disabled 用户禁用并移动到隔离 OU。

目录批次必须整体执行成功后才能确认。OU 环、重复 OU ID 等非法批次会失败确认，不推进本地 revision。

## 凭据执行

凭据响应包含中心当前密文在服务端内存解封后的明文密码。响应必须通过 TLS 传输，并设置 `Cache-Control: no-store`。Agent 不落盘明文密码，只在本轮立即通过 LDAPS Reset Password 设置到本域。

凭据批次整体成功后确认 `batch_revision`。任一密码设置失败时，Agent 发送失败 confirm，不推进本地凭据 revision，下轮继续重试。

## Confirm 请求

`POST /api/agent/confirm`

```json
{
  "domain_id": "domain-a",
  "channel": "credential",
  "target_revision": 9,
  "success": false,
  "error_code": "password_denied"
}
```

规则：

- `success=true` 时，中心推进对应通道的 applied revision。
- `success=false` 时，中心接受失败回报但不推进 revision。
- 服务端拒绝倒退确认。
- 服务端拒绝超过当前全局 revision 的确认。
- Agent 只有收到 `accepted=true` 才能保存本地 state。
