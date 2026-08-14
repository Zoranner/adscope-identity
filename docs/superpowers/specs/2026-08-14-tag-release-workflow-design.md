# 标签发布工作流设计

## 触发与版本契约

推送匹配 `v*` 的 Git 标签时触发发布。标签去掉 `v` 后必须与 `adscope-center` 和 `adscope-connector` 的统一 Cargo 版本完全一致；不一致时终止，不生成 Release。

## 发布产物契约

Release 包含以下四个文件：

- `adscope-connector-v<version>-windows-x86_64.zip`：Windows Connector 可执行文件、环境示例和服务安装说明。
- `adscope-center-v<version>-linux-amd64.tar`：Linux AMD64 Center Docker 镜像归档。
- `manifest.json`：版本、标签指向的 Git revision、目标平台与每个发布文件的 SHA-256。
- `SHA256SUMS`：与 manifest 一致的 SHA-256 清单。

## 执行边界

Windows runner 负责原生 Connector 编译与 ZIP 组装，Linux runner 负责 Center 镜像构建与归档。汇总任务仅在两类编译产物均成功后创建或更新同名 GitHub Release。

工作流只授予创建 Release 所需的 `contents: write` 权限，不推送容器镜像、不创建或移动 Git 标签，也不修改源码。相同标签重跑时覆盖该 Release 的同名附件，保证恢复发布不会保留旧产物。
