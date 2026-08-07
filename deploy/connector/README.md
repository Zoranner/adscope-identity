# Connector Windows 服务

发布包中的 `adss-connector.exe`、`.env.example`、安装脚本和卸载脚本应放在同一运行目录。安装脚本使用固定服务名 `ADStructureSyncConnector`，以 Windows 内置 `NetworkService` 身份运行，不依赖 WinSW 或 NSSM。

Connector 主机必须加入目标 AD 域。真实模式只配置 `ADSS_LDAP_URL=ldap://<FQDN>:389`，由主机计算机账号通过 Kerberos GSS-API 取得 LDAP 服务票据；不要配置或保存 LDAP bind DN、LDAP 密码、LDAP over TLS 或 StartTLS。域管理员仅向 `<DOMAIN>\<CONNECTOR-HOST>$` 委派镜像根和隔离 OU 的必要权限。

安装脚本会把运行目录、state 和日志限制给 `SYSTEM`、`Administrators` 和 `NetworkService`；`.env` 会移除 ACL 继承。`.env` 只保存域对应的 Connector key。

先根据 `.env.example` 创建 `.env` 并填写域对应的 Connector key。随后在管理员 PowerShell 中执行：

```powershell
pwsh -NoProfile -File .\install-service.ps1 -RuntimeDir 'C:\Program Files\AD Structure Sync Connector'
```

服务日志写入运行目录下的 `logs`，每天轮转并保留最近 14 个文件。同步 revision 保存在 `adss-connector-state.json`。

卸载服务时执行：

```powershell
pwsh -NoProfile -File .\uninstall-service.ps1
```

卸载脚本保留 `.env`、state 和 logs。确认不再需要这些运行数据后，由运维人员单独清理运行目录。
