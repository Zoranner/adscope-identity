# 发布交付收口设计

## 目标与约束

发布交付由两个独立单元组成：Center 作为 Docker 服务运行，Connector 作为原生 Windows 服务运行。

Center 固定使用 SQLite，并把数据库文件放在 Docker 持久化卷中。容器内部只提供 HTTP，由部署环境已有的反向代理统一终止 TLS。Connector 直接访问反向代理提供的 HTTPS 地址，并通过 LDAP 或 LDAPS 写入本地域控。

设计遵守以下约束：

- 不引入 PostgreSQL、CI、服务注册中心、密钥管理平台或独立发布服务。
- 不把 TLS 证书和生产密钥写入镜像、发布包或仓库。
- 不使用 WinSW、NSSM 等额外服务包装器。
- 不改变 Center 作为唯一事实源、Connector 主动拉取和双 revision 确认的业务模型。
- 不增加管理员账号、角色、会话、审计或独立 Connector key 轮换流程。

## 交付架构

Center 使用多阶段 Docker 构建。Web 构建阶段使用锁定的 Bun 依赖生成 Nuxt 静态文件，Rust 构建阶段使用锁定的 Cargo 依赖生成 Linux release 二进制，运行镜像只包含 Center、Web 静态文件和运行所需的系统库。

运行镜像使用非 root 用户，应用目录与数据目录分离：

- `/app/adss-center`：Center 二进制。
- `/app/web`：管理端与用户端静态文件。
- `/data/adss.db`：SQLite 数据库，由持久化卷提供。

Center 默认把 `ADSS_WEB_ROOT` 指向 `/app/web`，把 `ADSS_DATABASE_URL` 指向持久化卷中的 SQLite 文件。密码加密密钥、用户会话密钥和管理凭证只通过运行环境注入。

Connector 使用 `windows-service` 接入 Windows Service Control Manager。同一个 `adss-connector.exe` 同时支持控制台运行和服务运行，避免维护两套执行逻辑。服务安装命令显式指定运行目录，Connector 从该目录读取 `.env`、维护 revision state 并写入运行日志。

## Center 容器行为

Center 提供无需认证的健康检查接口。健康检查只验证进程和数据库连接，不返回配置、版本密钥或业务数据。数据库不可用时返回服务不可用状态，供 Docker 和反向代理判断实例是否可接收请求。

容器不终止 TLS，也不包含证书。反向代理必须保证：

- 外部管理入口和 Connector 同步入口只通过 HTTPS 提供。
- 不记录 `/api/connector/sync` 的响应体。
- 不缓存 Connector key 写响应和凭据同步响应。
- 只把内部 HTTP 端口暴露给受控容器网络。

SQLite 数据卷是 Center 的持久状态。备份必须在停止 Center 写入后执行，数据库文件与 `ADSS_PASSWORD_ENCRYPTION_KEY` 分开保存。恢复时必须同时使用原数据库和原密码加密密钥，否则已有密码密文无法解封。

## Connector 服务行为

Windows 服务注册名固定，显示名和描述由安装脚本统一配置。服务默认使用 `LocalService` 账户运行，安装脚本只授予该账户读取配置、写入 state 和日志所需的目录权限。

安装脚本不生成生产密钥，也不覆盖已有 `.env`。缺少 `.env`、必要变量为空或运行目录不可写时，安装或启动应明确失败。

服务接收停止控制后停止发起新一轮同步，等待正在执行的受控操作结束，并向 Service Control Manager 依次报告停止中和已停止状态。控制台模式继续支持本地 dry-run 和直接诊断。

Connector 对 Center HTTP 请求、LDAP 连接和单项 AD 操作设置明确超时。超时与其他执行失败均不得推进对应 revision，下轮同步继续处理当前事实。

## 错误与日志

Connector 使用结构化运行日志记录：

- 服务启动、停止和配置加载结果。
- 每轮同步的域标识、目录 revision、凭据 revision 和执行摘要。
- 失败操作的类型、目标稳定标识和脱敏后的底层错误。
- Center 请求失败、LDAP 连接失败、超时和 confirm 失败。

日志按天滚动并限制保留文件数量。Connector key、LDAP bind password、密码明文、密码密文和完整配置对象不得进入日志。包含秘密字段的配置类型不能使用自动派生的明文 `Debug` 输出。

Connector 向 Center 发送的失败确认继续使用稳定的通用错误码。详细底层错误只保留在本地运行日志中，不通过同步协议扩散。

## 发布产物

发布入口从干净工作树和锁文件构建，生成以下产物：

- Center Linux AMD64 Docker 镜像归档。
- Connector Windows x64 ZIP。
- 版本清单。
- SHA-256 校验文件。

Center 镜像标签、Connector crate 版本和版本清单使用同一个版本号。版本清单记录 Git commit、目标平台和每个交付文件的 SHA-256，不记录本机路径、密钥或环境变量值。

Connector ZIP 包含：

- `adss-connector.exe`。
- `.env.example`。
- Windows 服务安装脚本。
- Windows 服务卸载脚本。
- 包内运行说明。

发布入口只构建和组装本地产物，不创建 Git tag，不推送镜像，不上传 Release，也不修改远程状态。

## 安装、升级与回滚

Center 部署使用固定镜像标签和持久化卷。升级前停止写入并备份 SQLite 文件，加载新镜像后执行健康检查和登录、同步冒烟验证。失败时恢复旧镜像；如果 schema 已发生不兼容变化，同时恢复对应数据库备份和原加密密钥。

Connector 升级前停止服务，保留 `.env`、state 和日志，只替换二进制及安装脚本，再启动服务并检查服务状态和首轮同步日志。失败时停止服务、恢复旧二进制并重新启动。升级过程不得重置本地 revision state。

## 验证策略

实现采用测试驱动方式，验证范围包括：

- Center 健康检查在数据库可用和不可用时返回正确状态。
- Connector 执行失败保留可定位错误、发送通用失败确认且不推进 revision。
- HTTP、LDAP 和单项 AD 操作超时后结束本轮同步且不推进 revision。
- Windows 服务命令行、运行目录解析和停止信号能够复用同一同步循环。
- 发布脚本拒绝脏工作树或版本不一致，并生成完整 Connector ZIP、版本清单和校验文件。
- Docker 构建上下文包含 Center、path dependencies 和 Web 静态文件，不包含本机密钥和开发产物。
- Playwright 在最终 Center Web 上验证域新增和修改、一次性 Connector key、复制回退、焦点约束和保存期间离开保护。

真实发布验收还必须覆盖反向代理 TLS、代理日志与缓存、Windows Service Control Manager、SQLite 备份恢复，以及 AD 沙箱中的 OU、用户、组、成员、禁用、隔离移动和 Reset Password。自动化结果不能替代这些环境验收。

## 验证边界

缺少 Docker 运行时的开发环境只能验证 Dockerfile、构建上下文和发布脚本契约，不能宣称镜像已经构建或启动。缺少 AD 沙箱时只能验证 LDAP 协议、编码、执行顺序、错误和超时，不能宣称真实域控写入通过。
