# Adscope 首发改名设计

## 目标

产品正式名称为 `Adscope`。首个可交付版本固定为 `0.1.0`，不使用 `0.2.0-rc.1`。产品名称应表达 Active Directory 的治理范围，而非将能力限制为组织结构同步。

## 命名边界

`Adscope` 是唯一的对外产品标识。以下对象统一使用 `adscope` 或 `Adscope` 的适当大小写：仓库名称、README 和界面标题、Cargo package 与二进制、Docker 镜像、Windows 服务、环境变量、HTTP header、Cookie、日志、状态文件、SQLite 文件、发布包和部署文档。

Rust 内部 crate 名、环境变量、HTTP header 和文件名使用小写 `adscope`。面向管理员的界面、Windows 服务显示名和文档使用 `Adscope`。环境变量使用 `ADSCOPE_` 前缀。

## 兼容性

这是首发版本，不提供 `ADSS_*`、`x-adss-*`、`adss_*` Cookie、旧二进制名、旧服务名或旧发布包名的兼容入口。出现旧标识的配置应显式失败，不得静默回退。

会话、Connector key、密码加密和状态文件不做跨名称迁移。已有本地测试环境应以新的环境变量和运行目录重新配置；旧 SQLite 或 Connector state 不作为 `0.1.0` 的支持输入。

## 发布

版本统一为 `0.1.0`，并创建标注标签 `v0.1.0`。错误的本地标签 `v0.2.0-rc.1` 和对应 `dist/v0.2.0-rc.1` 构建产物删除。发布构建输出位于 `dist/v0.1.0`，其中的 manifest、校验和、Windows Connector ZIP 及 Linux Center 镜像归档均记录 `0.1.0` 和同一 Git revision。

完整发布仍要求有效 Git 远端和 Docker Linux AMD64 构建环境。缺少其中任一条件时，不得把本地 Windows Connector ZIP 表述为完整线上 Release。

## 验证

改名后，源码、部署文件和文档中不得保留有效 `ADSS` 或 `adss` 标识；历史提交和旧构建目录不属于该扫描范围。Rust workspace、独立 protocol/store crate、前端静态构建、Docker/服务脚本/发布契约均使用新名称验证。发布包需要核验内部文件名、manifest revision、版本号和 SHA256。
