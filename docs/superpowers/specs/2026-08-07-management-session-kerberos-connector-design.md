# 管理会话与 Kerberos Connector 设计

## 目标与约束

管理台在浏览器刷新后保持已登录状态，但部署级管理密钥不得持久化到浏览器。Connector 通过域控的 LDAP 389 端口执行目录同步和密码重置，不使用 LDAPS、不保存 LDAP 服务账号密码、不以域超级用户运行。

设计不引入管理员表、角色体系、服务端会话表、gMSA、普通域服务账号、审计表或新的同步协议。Center 继续作为唯一事实源，Connector 继续主动拉取并通过 revision confirm 记录成功结果。

## 管理会话

`ADSS_MANAGEMENT_TOKEN` 仅用于建立管理会话。管理页首次提交该值后，Center 校验 token 并签发独立的短期会话 Cookie。Cookie 使用 `HttpOnly`、`Secure`、`SameSite=Strict` 和受限 API Path；浏览器脚本不能读取 Cookie 或管理 token。

会话由现有会话根密钥派生出独立用途密钥后签名，携带到期时间和随机 CSRF nonce。Center 不保存会话记录。页面加载时调用会话查询接口恢复管理状态和当前 CSRF nonce；写操作必须同时带有效 Cookie 与匹配的 CSRF 请求头。退出接口使浏览器删除 Cookie，前端同时清除内存中的 CSRF nonce。

浏览器管理台只使用 Cookie 会话。现有 Bearer 管理 token 入口保留给非浏览器自动化调用，避免改变外部接口的认证方式。

## Connector 认证与传输

真实 Connector 配置只接受 `ldap://<domain-controller-fqdn>:389`。URL 主机必须是域控完整域名，不能使用 IP 地址，以便 Kerberos 将其匹配到 LDAP 服务主体。Connector 启用 `ldap3` 的 GSS-API 支持，并以进程默认 Windows 凭据执行 SASL GSS-API bind。

无 TLS 的 GSS-API bind 必须协商 Kerberos 保密层；协商失败时，本轮同步失败且不推进 revision。Connector 不提供 Simple Authentication、NTLM、LDAPS、StartTLS 或降级回退路径。`.env` 不包含 LDAP bind DN、bind password 或证书跳过校验配置。

目录同步批次和凭据同步批次各建立一个已绑定 LDAP 连接。连接在该批次结束后关闭；批次内所有顺序操作复用同一连接。任一连接、Kerberos 或 LDAP 操作失败时，保留现有失败确认与下轮重试语义。

## Windows 服务身份与 AD 委派

Connector Windows 服务使用内置 `NT AUTHORITY\NetworkService` 运行。该账户本地权限较低，访问域控时以 Connector 主机已有的 AD 计算机账号认证，例如 `RD\CONNECTOR-PC$`。域成员关系和计算机账号均为部署前提，不由应用创建或维护。

域管理员将镜像根 OU 和隔离 OU 的最小权限委派给该计算机账号：受管 OU、用户和组的创建、必要属性写入、成员关系修改、禁用、在受管根与隔离 OU 之间移动，以及 Reset Password。该计算机账号不属于 `Domain Admins`，不得被授予受管 OU 以外的目录管理权限。服务不得以 `superuser` 或其他高权限人工账号运行。

安装脚本以 `NetworkService` 注册服务，并只授予其读取 Connector 配置、写入本地 revision state 和日志的权限。`.env` 禁用继承访问控制，仅保留 `SYSTEM`、本地 Administrators 和服务运行身份的必要访问权限。

## 数据一致性与批处理

用户目录记录和初始凭据在同一个持久化事务内创建，任一步失败均不遗留可见的半创建用户。目录同步按 `batch_limit` 确定上限、顺序和 `has_more`，Connector 仅确认已完整执行的目标 revision，避免首次重建生成无界响应。

## 验证与运行前提

实现遵循测试驱动：覆盖管理会话的刷新恢复、Cookie 属性、CSRF 和退出；Connector 配置拒绝旧的 Simple Authentication 字段与 IP LDAP 主机；LDAP 客户端对每个批次只绑定一次；服务脚本使用 `NetworkService` 并收紧配置 ACL；事务失败不留下目录记录；分页边界与 confirm 语义保持一致。

本地验证包含 Rust 格式化、Clippy、workspace 与独立 protocol/store crate 测试、Bun 的类型检查和测试、以及 Docker、Connector 服务脚本和发布组装的 PowerShell 契约测试。真实域环境验收必须确认域控 FQDN 解析、Connector 主机域成员关系、计算机账号 OU 委派、GSS-API 保密层、目录对象写入和 Reset Password。自动化测试不能替代这些环境验证。
