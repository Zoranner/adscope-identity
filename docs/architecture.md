# AD 多域组织结构同步架构

## 目标

系统采用中心主服务与域内 Agent 的控制面/执行面分离架构。主服务是组织结构、用户生命周期、组成员关系、密码密文、同步版本和审计的唯一事实源；每个 AD 域内运行本地 Agent，由 Agent 主动轮询主服务并通过 LDAPS 幂等更新本地域。

首版只覆盖同步核心能力：组织结构、用户、组、成员关系、密码下发、结果回传与 drift 告警。不纳入完整 Web 管理平台、审批流、HR 对接、计算机账号、GPO 或证书同步。

## 拓扑

```text
管理员 / 用户
  |
  v
中心主服务 API
  |
  v
中心数据库 / 事实源
  ^
  |
AD 域内 Agent 主动轮询
  |
  v
本地域控 AD
```

主服务不主动访问域控，也不要求能访问 Agent 的内网地址。Agent 绑定一个 `domain_id`，只能拉取本域任务。一个域可以部署多个 Agent，但同一时间的任务领取和 cursor 推进必须由主服务保证幂等。

## 领域对象

- `User`：以 `employee_id` 为全局唯一主键，包含 `sam_account_name`、`upn`、显示名、状态、目标 OU 和白名单属性。
- `OrgUnit`：中心组织树节点，映射为域镜像根下的相对 DN。
- `Group`：组定义与 AD 侧 `sAMAccountName`。
- `GroupMembership`：组与用户的成员关系。
- `Domain`：域配置、镜像根 DN、隔离 OU、工号属性映射和 Agent 策略。
- `StructureVersion`：组织、用户、组、成员关系的目标状态版本。
- `PasswordTask`：面向单个域的密码下发任务，不在接口响应中暴露明文。
- `SyncRun`：Agent 一次执行的结果摘要。
- `AuditEvent`：记录人、Agent、对象、动作、结果与时间。

## 同步执行

组织结构采用版本化 desired state reconcile。Agent 上报本地 cursor，主服务根据版本返回无变化、增量或完整快照。Agent 执行顺序固定为 OU、组、用户、用户位置、组成员、密码任务、删除隔离策略。

密码修改采用高优先级任务队列。用户在主服务改密后，主服务为每个已接入域创建独立 `PasswordTask`。单域失败不影响其他域，失败原因必须回传并可重试。

AD 手工变更只作为 drift 回传和告警，不自动合并回主服务。中心服务始终是单一事实源。

Agent runtime 的一轮执行流程为 `poll -> desired state reconcile -> password tasks -> report`。运行时通过 `ControlPlaneClient` 对接主服务，通过 `DirectoryClient` 对接 AD；这两个边界允许测试中使用内存或 dry-run 实现，生产中替换为 HTTP/mTLS 和 LDAPS 实现。

## 删除与离职

用户删除或离职不物理删除 AD 对象，默认禁用账号并移动到域配置的隔离 OU。组和 OU 的破坏性删除首版不自动执行，只标记为 drift 或 pending destructive change，等待后续显式确认机制。
