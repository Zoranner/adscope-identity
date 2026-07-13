# AD 多域同步 MVP 设计

## 目标与边界

本 MVP 建立一条可实际运行的单向同步主链：

```text
中心维护当前事实
→ 每个域的 Agent 定时主动拉取
→ Agent 通过 LDAPS 幂等写入本域 AD
→ 成功后确认本轮 revision
```

确定边界如下：

- 中心服务是 OU、用户、组、成员关系和密码的唯一修改入口。
- 中心数据库是唯一事实源，服务内存不保存可独立变化的业务副本。
- AD 域只做下游，不向中心或其他域传播修改。
- 每个域只部署一个 Agent，不设计同域多 Agent 协调。
- Agent 常驻运行并定时主动请求，中心不主动连接域控。
- 目录信息和凭据使用两个独立同步通道，独立执行、独立确认、互不阻塞。
- 正常传输只包含修改过的对象，但每个对象携带完整当前状态。
- 不保存目录变更日志、凭据变更队列、旧对象状态或旧密码。
- MVP 不自动删除 OU、组或用户。

## 最终状态增量模型

中心只保存当前事实，并通过对象上的 `changed_revision` 判断某个域尚未应用哪些对象。

目录通道包含 OU、用户、组及其完整成员集合；凭据通道只包含用户当前密码材料。两个通道分别维护当前 `directory_revision`、当前 `credential_revision` 和每个域已确认的两个 applied revision。

系统不保存“从旧状态变成新状态”的事件。Agent 获取满足以下条件的当前对象：

```text
object.changed_revision > domain.applied_revision
```

对象在 Agent 离线期间被多次修改时，只传输最后的完整当前状态，不重放中间状态。密码同样只下发该用户最新密码。

revision 允许存在空洞。revision 表示中心事务的提交批次，不要求每个数字都对应当前仍可查询到的对象。

## 事实模型

### OU

`organizational_units` 保存 `id`、`name`、`parent_id` 和 `changed_revision`。`id` 是改名或移动时不变的中心稳定标识，根 OU 的 `parent_id` 为空。

OU 的域内 DN 由父子关系和域的 `mirror_root_dn` 推导，不重复保存完整 DN。

### 用户

`users` 保存以下最小字段：

- `employee_id`：跨域唯一工号，也是中心用户主键。
- `username`：各域一致的 `sAMAccountName`。
- `display_name`、`email`、`mobile`、`telephone`：用户白名单资料，后三项可为空。
- `organizational_unit_id`：用户正常启用时所属 OU。
- `status`：只允许 `active` 或 `disabled`。
- `changed_revision`：最后一次目录修改 revision。

完整 UPN 由 `username + "@" + domain.upn_suffix` 推导，不在用户表重复保存。AD 中通过统一工号属性定位用户，不以用户名、DN 或各域不同的 `objectGUID` 作为跨域主键。

### 组

`groups` 固定保存 `id`、`name`、`member_employee_ids` 和 `changed_revision`。`name` 同时映射为 CN 和 `sAMAccountName`，`member_employee_ids` 是当前完整用户成员集合。

MVP 中所有组固定为全局安全组，不开放组类型、作用域或名称映射配置。成员关系归组保存，不建立独立 `group_memberships` 实体。

### 用户凭据

`user_credentials` 保存 `employee_id`、可下发密码的 `password_ciphertext`、中心登录使用的不可逆 `password_verifier` 和最后一次改密的 `changed_revision`。

`password_ciphertext` 用于解密后向 AD Agent 下发当前密码；`password_verifier` 用于中心登录时验证用户输入，不能还原密码。两者职责不同，不是重复事实。

一次改密只保留该用户最新的密文和 verifier，不保留旧密码，不重放旧密码。

### 域

`domains` 保存：

- `id`、`name`、`enabled`：域标识、展示名称和启用状态；`id` 也是唯一 Agent 绑定标识。
- `mirror_root_dn`、`quarantine_ou_dn`：受管目录树根和 disabled 用户隔离 OU。
- `upn_suffix`、`employee_id_attribute`：本域 UPN 后缀和工号属性名。
- `agent_key_hash`：共享 Agent key 的安全哈希。
- `applied_directory_revision`、`applied_credential_revision`：该域已确认的两个 revision。

域记录直接保存两个 applied revision，不建立 `domain_sync_states`。最近运行时间和错误属于可选运行日志，不进入 MVP 核心事实模型。

AD 地址、绑定账号和绑定密码只保存在域内 Agent，不进入中心数据库。

### 同步元数据

中心保存一条最小同步元数据记录，只包含当前已提交的 `directory_revision` 和 `credential_revision`。

元数据使用单行表或数据库等价机制，不扩展为业务实体或事件流。

## revision 规则

### 目录 revision

一次中心目录事务只分配一个新的 `directory_revision`。

同一事务可以同时修改多个 OU、用户和组。所有被修改对象写入相同的 `changed_revision`，然后提交新的全局 `directory_revision`。

以下内容必须在同一数据库事务中完成：

```text
读取并递增 directory_revision
→ 更新全部受影响对象及 changed_revision
→ 提交事务
```

事务失败时，对象状态和全局 revision 都不变化。

### 凭据 revision

一次用户改密事务只分配一个新的 `credential_revision`，并同时更新：

- 该用户的 `password_ciphertext`。
- 该用户的 `password_verifier`。
- 该用户的 `changed_revision`。
- 全局 `credential_revision`。

一次事务只修改一个用户密码。事务失败时不保留部分密码数据，也不推进 revision。

### 批次边界

中心按 `changed_revision` 升序查询并分页，但不得拆分同一个 revision 的对象集合。

每次响应包含 `target_revision`，它是本轮返回对象中最大的 `changed_revision`。如果没有待同步对象，`target_revision` 等于 Agent 请求中的 applied revision。

Agent 不逐条确认 revision，而是对本轮整批确认。revision 有空洞不影响确认。

## 同步协议

### 拉取请求

Agent 每轮请求携带：

```text
domain_id
applied_directory_revision
applied_credential_revision
rebuild_directory
rebuild_credentials
```

两个 `rebuild` 标志默认 false，只在 Agent 本地状态损坏或运维明确要求重建时使用。

中心校验：

- 域存在、启用且共享 key 匹配。
- 请求中的 applied revision 不高于中心当前 revision。
- 请求中的 applied revision 不高于中心已确认的域 revision。
- Agent 请求值小于中心已确认值时允许幂等重做，不以中心值强行跳过。

### 目录响应

正常模式查询：

```text
organizational_units.changed_revision > applied_directory_revision
users.changed_revision > applied_directory_revision
groups.changed_revision > applied_directory_revision
```

响应包含：

- `target_revision`。
- 修改过的 OU 完整当前状态。
- 修改过的用户完整当前状态。
- 修改过的组及完整当前成员集合。
- `has_more`，表示当前 revision 之后仍有对象。

首次同步时 applied revision 为零，仍使用相同查询和分页规则，不需要独立 snapshot 模型。

`rebuild_directory=true` 时，中心忽略 applied revision，返回全部当前 OU、用户和组；该响应只是一种全量查询，不落库为 snapshot。

分页必须保持依赖安全。中心按以下顺序组装本轮对象：

```text
OU
→ 用户
→ 组及成员集合
```

如果批次容量不足以容纳同一 revision 的全部对象，必须扩大该批次，不能拆分该 revision。

### 凭据响应

正常模式查询：

```text
user_credentials.changed_revision > applied_credential_revision
```

响应包含本轮 `target_revision`、用户工号和当前密码明文载荷。只返回已认证 Agent 所属域需要应用的当前有效用户凭据。

首次同步时 applied revision 为零，按相同过滤和分页规则返回当前凭据。

`rebuild_credentials=true` 时返回全部当前有效用户凭据，不建立或读取旧密码历史。

目录响应和凭据响应使用独立字段或独立接口，凭据内容不得混入目录对象。

### 整批确认

目录和凭据分别回报：

```text
domain_id
channel
target_revision
success
error_code
```

目录批次全部成功时，中心将域的 `applied_directory_revision` 更新为 `target_revision`。任一目录对象失败时，本轮目录 revision 不推进。

凭据批次全部成功时，中心将域的 `applied_credential_revision` 更新为 `target_revision`。任一密码设置失败时，本轮凭据 revision 不推进。

中心只接受不倒退、不超过当前全局 revision 的确认。重复确认同一 revision 必须幂等成功。

不建立待确认批次、下发记录、租约或 report 历史实体。

## Agent 执行与恢复

Agent 常驻执行：

```text
读取本地两个已确认 revision
→ 请求中心
→ 执行目录批次
→ 目录全部成功后回报并保存目录 revision
→ 执行凭据批次
→ 凭据全部成功后回报并保存凭据 revision
→ 等待下一轮
```

两个通道独立执行和回报：

- 目录失败不阻止本轮凭据执行。
- 凭据失败不回退已确认的目录 revision。
- 每个通道只在中心确认响应成功后原子更新本地已确认值。

如果 report 网络失败，Agent 不更新该通道本地 revision。下一轮继续用旧 revision 请求，中心重新返回当前受影响对象，Agent 依靠幂等操作整批重做。

Agent 本地只保存两个 revision，不保存密码、目录对象或待处理任务。文件必须原子替换，无法解析时将对应通道设为重建模式。

中心服务重启后直接从数据库读取当前事实和域进度。不得通过内存种子重建生产状态。

## 目录执行规则

Agent 通过工号属性识别用户，通过中心稳定 ID 和受管根范围识别 OU 与组。

目录批次按以下顺序执行：

```text
确保 OU 层级
→ 确保用户存在并同步完整白名单属性
→ 将 active 用户移动到 organizational_unit_id 对应 OU
→ 确保组为全局安全组并同步名称
→ 将受管用户成员关系对齐到 member_employee_ids
→ 禁用 disabled 用户并移动到 quarantine_ou_dn
```

用户从 `disabled` 恢复为 `active` 时，Agent 启用账号，并根据 `organizational_unit_id` 将其移回正常 OU。

MVP 不自动删除 AD 用户、OU 或组。中心也不提供物理删除主链；停用用户使用 `disabled`。

Agent 只能操作 `mirror_root_dn`、`quarantine_ou_dn` 和能够确认归属的受管对象，不接管同名但无法确认标识的域内对象。

## 密码与登录安全

### 中心登录与改密

普通用户通过中心最小登录接口提交工号和当前密码。中心使用 `password_verifier` 验证身份，登录成功后只允许用户修改自己的密码。

改密成功后，中心生成新的 verifier 和加密密码材料，并推进 credential revision。

管理员代设密码、角色平台、审批、到期提醒和复杂会话治理不进入 MVP。

### 密码存储与下发

中心数据库保存加密密码材料，不保存明文密码。加密密钥与数据库分离管理。

凭据接口处理流程确定为：

```text
校验 Agent key 与域绑定
→ 在中心进程内存中解密当前密码
→ 通过 TLS 的独立凭据响应下发明文
→ 响应完成后释放明文缓冲
```

必须实施：

- 凭据响应禁止 HTTP、代理和 CDN 缓存。
- 禁止记录请求或响应正文。
- 禁止 trace body、调试转储和错误回显密码。
- 错误日志只记录域、工号和非敏感错误码。
- Agent 不把明文或密文密码写入磁盘。
- Agent 收到密码后立即通过 LDAPS 执行 Reset Password，并清理内存引用。

MVP 使用每域预置的高熵共享 Agent key 认证，中心只保存 key 哈希，传输必须使用 TLS。mTLS 和密钥轮换平台延后。

### 禁止域内普通改密

受管用户必须同时配置：

- GPO 隐藏 `Ctrl+Alt+Del` 的“更改密码”。
- AD 对象权限禁止用户执行 Change Password。
- 不设置“下次登录必须修改密码”。

Agent 服务账号只获得受管范围内必要的目录写权限和 Reset Password 权限，不使用域管理员账号。

## 范围外

MVP 不包含：

- 多 Agent、选主、租约和高可用协调。
- `directory_changes`、`credential_changes` 或按域复制的任务表。
- 旧对象状态、旧密码、事件重放和事件溯源。
- 反向同步、域到域传播和 drift 实体。
- 自动删除 OU、组或用户。
- 对象级 report 历史、完整审计平台和审批平台。
- 管理员代设密码、角色平台、提醒和密码到期工作流。
- 任意 LDAP 属性字典和通用目录建模。
- mTLS、证书注册和复杂密钥轮换。
- 通用消息队列、工作流引擎或事件总线。

## 验收标准

- 中心数据库只保存当前 OU、用户、组、域、当前凭据和两个全局 revision。
- 不存在目录变化日志、凭据变化队列、旧密码或独立域同步状态实体。
- 一次目录事务只分配一个 revision，全部受影响对象写相同 `changed_revision`。
- 一次改密只保存该用户最新密文和 verifier，并分配一个 credential revision。
- Agent 正常请求只获得 `changed_revision > applied_revision` 的当前完整对象。
- 首次 revision 为零时按正常过滤分页返回；显式重建时返回全部当前对象。
- 同一 revision 的对象不会被分页拆分。
- 每个通道整批成功才推进到 `target_revision`，任一失败不推进。
- report 网络失败时 Agent 不更新本地 revision，下一轮能安全幂等重做。
- 目录与凭据独立执行、独立确认，任一通道失败不阻塞另一通道。
- disabled 用户在各域被禁用并移入隔离 OU，恢复 active 后移回正常 OU。
- 用户名、邮箱、手机、电话、OU、组和成员集合修改能同步到至少两个独立测试域。
- 中心改密后，两个域使用 LDAPS Reset Password 得到相同新密码。
- 普通用户能登录中心并修改自己的密码，不能通过域内普通方式改密。
- 密码明文不进入数据库、磁盘、缓存、日志、trace 或错误响应。
- 中心和 Agent 重启后从持久化 revision 继续，不丢状态、不跳 revision。
- 全链路不依赖 dry-run；真实 LDAPS 同步和失败重试通过验证。
