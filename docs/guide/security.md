# 安全要求

## 密码事实源

用户密码通过中心服务修改，并由各域 Connector 同步到本地域控。域内 AD 的普通 Change Password 路径不作为事实源，也不会传播到其他域。

受管账号应禁止用户在 AD 中自行 Change Password，并通过 GPO 隐藏 `Ctrl+Alt+Del` 的“更改密码”入口。Connector 使用被委派的 Reset Password 权限设置中心下发的密码。

## 密钥和配置

主服务必须配置高熵 `ADSS_PASSWORD_ENCRYPTION_KEY`、`ADSS_USER_SESSION_KEY` 和 `ADSS_MANAGEMENT_TOKEN`。这些密钥通过受限 `.env`、系统环境变量、Windows DPAPI、Secret Manager 或等价机制注入，不进入源码、日志或配置仓库。

Connector key 和 LDAP bind password 按域独立保存。Connector key 运行时不得写入日志、错误响应或配置仓库。

## 传输要求

主服务必须位于 TLS 后面。Connector 调用 `/api/connector/sync` 获取凭据材料时，响应不能经过明文 HTTP，代理、网关、日志、tracing、错误回显和崩溃 dump 不记录响应体。

Connector 访问域控支持 `ldap://` 或 `ldaps://`。生产环境建议使用 `ldaps://`，或仅在受保护网络内使用 `ldap://`；如果域控策略要求加密密码修改，应按域策略启用 `ldaps://` 或等价受保护绑定。

## 权限边界

普通用户 token 只用于本人资料和本人改密接口。普通用户不能选择其他用户，也不能修改工号、账号名、显示名、组织归属、启用状态、组成员或域配置。

管理入口必须使用独立保护，不能复用普通用户 token。域、OU、用户、组、管理员代设密码和同步状态查询接口都属于管理面。

域内服务账号采用最小权限委派，只允许管理镜像根和隔离 OU 内的目标对象，并授予必要的创建、移动、属性写入、组成员写入、禁用和 Reset Password 权限。
