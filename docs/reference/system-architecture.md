# AD 多域组织结构同步架构

## 设计目标

系统以中心服务作为账号与目录事实源，把 OU、用户、组、组成员关系和密码同步到多个独立 AD 域。每个域部署一个 Connector，Connector 主动从中心服务拉取期望状态并写入本地域控。

架构优先保证主链清晰：

- 中心数据库保存权威事实。
- 主服务负责 API、鉴权、密码保护和同步控制面。
- Connector 只执行本域同步，不反向修改中心事实。
- 目录和凭据分通道同步，互不阻塞。
- 同步基于当前状态，不重放历史事件。

## 系统组成

```text
普通用户入口
管理员入口
  |
  v
中心主服务 API
  |
  v
中心数据库
  ^
  |
域内 Connector 定时 sync / confirm
  |
  v
本地域控 AD
```

主服务不主动连接域控，也不要求访问 Connector 内网地址。Connector 保存本地域的本地 revision state 和 Connector key，通过固定轮询访问中心服务。

## Crate 职责

| Crate | 职责 |
| --- | --- |
| `adss-protocol` | 定义同步对象、请求响应、目录执行计划和共享错误边界。 |
| `adss-store` | 封装数据库模型、事实源读写和 revision 更新规则。 |
| `adss-center` | 提供中心 API、用户会话、Connector 鉴权、密码加密和同步控制面。 |
| `adss-connector` | 管理 Connector 配置、本地 state、HTTP 拉取确认和域控写入。 |

crate 拆分只服务明确边界：共享契约、持久化、中心服务和域内执行。没有独立生命周期或复用价值的逻辑不单独拆 crate。

## 事实模型

`organizational_units` 保存中心 OU 树。节点使用稳定 `id` 和 `parent_id` 表示父子关系。Connector 在域配置的 `mirror_root_dn` 下创建对应 OU 层级。

`users` 保存用户目录事实。`employee_id` 是跨域唯一标识；`username`、显示名、邮箱、手机、办公电话、目标 OU 和状态属于目录通道。禁用用户仍是受管用户，Connector 先确保用户存在，再禁用并移动到 `quarantine_ou_dn`。

普通用户自助只允许修改 `email`、`mobile` 和 `telephone`。`employee_id`、`username`、显示名、目标 OU、状态和组成员由管理员入口或上游权威流程维护。

`groups` 保存组当前状态。成员集合直接保存在组记录中，不单独建成员实体。组名映射为 AD 组 CN 和账号名。

`user_credentials` 保存当前凭据事实。每个用户只保存当前 verifier 和 ciphertext，不保存旧密码历史，也不创建按域拆分的密码任务。

`domains` 保存域配置、Connector key hash、启用状态和该域已确认的目录/凭据 revision。

普通用户会话不写入数据库。主服务登录成功后签发服务端签名的短期 Bearer token，`/api/me/*` 接口从 token 中确定当前 `employee_id`。

## Revision 模型

目录通道使用 `directory_revision`。任意 OU、用户、组或组成员变化都会在同一个中心事务中分配新的目录 revision，并写入受影响对象的 `changed_revision`。

凭据通道使用 `credential_revision`。每次中心改密或管理员重置密码分配新的凭据 revision，并写入该用户当前凭据的 `changed_revision`。

Connector 拉取最终当前状态：

- 目录拉取返回 `changed_revision > applied_directory_revision` 的当前 OU、用户和组。
- 凭据拉取返回 `changed_revision > applied_credential_revision` 的当前密码材料。
- rebuild 请求把对应通道进度视为 `0`，重新拉取当前事实。

系统不保存目录变更日志、凭据变更队列、旧对象状态或旧密码。对象在 Connector 离线期间被多次修改时，Connector 只收到最新完整状态。

## 同步执行

Connector 每轮执行：

```text
读取本地 applied revisions
→ POST /api/connector/sync
→ 执行目录批次
→ 目录成功后 POST /api/connector/confirm
→ 执行凭据批次
→ 凭据成功后 POST /api/connector/confirm
→ confirm accepted 后写本地 state
→ 等待下一轮
```

目录计划由契约层生成，顺序为父 OU、用户、用户位置、组、组成员、禁用用户隔离。凭据批次独立执行，凭据失败不会回退目录确认。

`server_revision` 表示中心当前全局 revision，`batch_revision` 表示本批允许确认的最高 revision。Connector 只能在整批成功后确认 `batch_revision`。

## 接口边界

HTTP API 按身份和职责分为三类：

- 普通用户自助：登录、查看本人资料、修改本人联系方式、修改本人密码。
- 管理入口：维护域、OU、用户、组、密码重置和同步状态。
- Connector 同步：按域拉取目录和凭据批次，并确认执行结果。

普通用户 token 只进入 `/api/me/*`。管理员接口必须使用独立管理凭证，不能复用普通用户 token。Connector 接口只接受域绑定 Connector key，并只能访问该域的配置和同步数据。

rebuild 由 Connector 主动携带标志触发。

## 不处理的范围

以下内容不属于本系统范围：

- Connector 注册流程。
- 复杂角色平台、审批流和密码到期提醒。
- 任务队列和任务领取。
- 历史事件重放、事件溯源和旧密码保存。
- 多 Connector 协调。
- 物理删除 OU、组或用户。
- 域内人工修改反向同步。
