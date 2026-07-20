# AD 多域组织结构同步架构

## 目标

系统用于把中心服务维护的组织结构、用户信息、组成员关系和密码同步到多个独立 AD 域。中心服务是唯一事实源，各域通过本地域内 Agent 定时主动拉取并执行。

MVP 只保留最小可用主链：

- 中心维护当前目录事实和当前凭据事实。
- 每个域一个 Agent 主动拉取。
- 目录和凭据按独立通道同步。
- Agent 成功执行整批后确认 revision。
- 域内人工修改不反向合并到中心。

## 拓扑

```text
用户 / 管理入口
  |
  v
中心主服务 API
  |
  v
中心数据库
  ^
  |
域内 Agent 定时 sync / confirm
  |
  v
本地域控 AD
```

主服务不主动访问域控，也不要求能访问 Agent 内网地址。Agent 只保存本地域的本地 revision state 和认证密钥。

## 当前事实模型

`organizational_units` 保存中心组织节点。节点使用稳定 `id` 和 `parent_id` 表达树关系，Agent 在域配置的 `mirror_root_dn` 下生成 AD OU。

`users` 保存用户目录事实。`employee_id` 是跨域唯一标识，`username`、显示名、邮箱、手机、电话、目标 OU 和状态都属于目录通道。`disabled` 用户仍是受管用户，Agent 先确保用户存在，再禁用并移动到 `quarantine_ou_dn`。

`groups` 保存组当前状态。成员集合直接保存在组记录中，不单独建成员实体。MVP 中组名固定映射为 AD 组 CN 和账号名。

`user_credentials` 保存当前凭据事实。每个用户只保留当前 verifier 和 ciphertext，不保留旧密码历史，也不创建按域复制的密码任务。

`domains` 保存域配置、Agent key hash 和该域已确认的目录/凭据 revision。

## Revision 模型

目录通道使用 `directory_revision`。任意 OU、用户、组或组成员变化都会在同一个中心事务中分配新的目录 revision，并写入受影响对象的 `changed_revision`。

凭据通道使用 `credential_revision`。每次中心改密分配新的凭据 revision，并写入该用户当前凭据的 `changed_revision`。

Agent 拉取的是最终当前状态，不重放旧中间状态：

- 目录拉取返回 `changed_revision > applied_directory_revision` 的当前 OU、用户和组。
- 凭据拉取返回 `changed_revision > applied_credential_revision` 的当前密码材料。
- rebuild 请求把对应通道进度视为 `0`。

## 同步执行

Agent 每轮执行：

```text
读取本地 applied revisions
→ POST /api/agent/sync
→ 执行目录批次
→ 目录成功后 POST /api/agent/confirm
→ 执行凭据批次
→ 凭据成功后 POST /api/agent/confirm
→ confirm accepted 后写本地 state
→ 等待下一轮
```

目录计划由契约层生成，顺序为父 OU、用户、用户位置、组、组成员、disabled 用户禁用和隔离。凭据批次独立执行，失败不会阻止目录通道确认。

`server_revision` 表示中心当前全局 revision，`batch_revision` 表示本批允许确认的最高 revision。Agent 只能在整批成功后确认 `batch_revision`。

## 简化边界

MVP 不包含：

- Agent 注册流程。
- 任务队列和任务领取。
- 旧 poll/report cursor 模型。
- drift 上报与生命周期。
- 多 Agent 协调。
- 物理删除 OU、组或用户。
- 域内人工修改反向同步。
- 生产 KMS、mTLS 和真实 AD 沙箱验收。

这些能力只能在主链稳定后按明确需求单独设计和验证。
