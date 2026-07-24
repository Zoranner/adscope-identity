# Connector 同步协议

## 协议边界

每个域部署一个 Connector。Connector 定时主动请求中心服务，中心服务不主动连接域控或 Connector。

Connector 请求使用 `x-adss-connector-key` 认证，只能访问请求 `domain_id` 对应的域配置和同步数据。中心服务返回的是当前期望状态；域内 AD 是下游执行目标。

## 同步通道

同步拆成两个独立通道：

- `directory`：OU、用户资料、用户状态、用户目标 OU、组、组目标 OU 和组成员。
- `credential`：中心当前密码材料。

两个通道分别拉取、执行和确认。目录失败不阻塞凭据执行，凭据失败不回退目录确认。

## Sync 请求

`POST /api/connector/sync`

```json
{
  "domain_id": "domain-a",
  "applied_directory_revision": 10,
  "applied_credential_revision": 7,
  "rebuild_directory": false,
  "rebuild_credentials": false
}
```

`applied_*_revision` 表示 Connector 本地已经成功执行并被中心接受的 revision。请求 revision 不能高于中心为该域记录的 applied revision；如果上次 confirm 响应丢失，Connector 应先重试 confirm。

`rebuild_directory=true` 表示目录通道从 revision `0` 重新拉取当前对象。`rebuild_credentials=true` 表示凭据通道从 revision `0` 重新拉取当前凭据。Connector 本地 state 文件无法解析时，可通过两个 rebuild 标记恢复。

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
    "employee_id_attribute": "employeeID",
    "managed_group_id_attribute": "adminDescription"
  }
}
```

`server_revision` 是中心全局 revision。`batch_revision` 是本次响应允许 Connector 确认的最高 revision。Connector 只能确认 `batch_revision`，不能直接确认未返回的更高版本。

目录通道可以整批返回。凭据通道可以按批返回，服务端必须保证 `batch_revision` 不越过本批实际返回范围。

## 目录执行

目录响应包含 changed revision 大于请求进度的当前对象状态。Connector 不重放旧中间状态。

Connector 将目录批次转为固定执行顺序：

- 父 OU 先于子 OU。
- 确保用户存在并同步属性。
- active 用户移动到目标 OU。
- 确保组存在并位于目标 OU。
- 同步组成员集合。
- disabled 用户禁用并移动到隔离 OU。

目录批次必须整体执行成功后才能确认。OU 环、重复 OU ID、缺失父 OU、用户或组目标 OU 不存在等非法批次应失败确认，不推进本地 revision。

## AD 字段边界

Connector 按中心下发的域配置写入 AD，不在本地 `.env` 配置字段映射。

用户工号写入 `employee_id_attribute` 指定的属性，默认 `employeeID`。受管安全组写入 `managed_group_id_attribute` 指定的属性，默认 `adminDescription`，属性值格式为 `adss:group:{group.id}`。Connector 通过该属性在受管范围内查找已有组对象。

标准 AD 字段保持固定映射：用户账号名写入 `sAMAccountName` 和 UPN 本地部分，显示名写入 `displayName`，邮箱写入 `mail`，手机号写入 `mobile`，办公电话写入 `telephoneNumber`，密码写入 `unicodePwd`。组名写入 `cn` 和 `sAMAccountName`，组成员写入 `member`。

## 凭据执行

凭据响应包含中心当前密文在服务端内存解封后的明文密码。Connector 调用主服务 `/api/connector/sync` 时必须走 TLS，并且响应必须设置 `Cache-Control: no-store`。

Connector 不落盘明文密码，只在本轮立即通过域控连接执行 Reset Password。域控连接支持 `ldap://` 或 `ldaps://`；生产环境建议使用 `ldaps://`，或仅在受保护网络内使用 `ldap://`。密码下发和管理面仍必须通过主服务 TLS。任一密码设置失败时，Connector 发送失败 confirm，不推进本地凭据 revision，下轮继续重试。

## Confirm 请求

`POST /api/connector/confirm`

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
- Connector 只有收到 `accepted=true` 才能保存本地 state。

响应：

```json
{
  "accepted": true
}
```
