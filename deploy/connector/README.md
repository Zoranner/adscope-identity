# Connector Windows 安装包

发布包是 `adscope-connector-v<version>-windows-x64.msi`。安装包使用固定服务名 `AdscopeConnector`，以 Windows 内置 `NetworkService` 身份运行，不依赖 WinSW 或 NSSM。

Connector 主机必须加入目标 AD 域。配置 `DOMAIN_CONTROLLER_LDAP_URL=ldap://<FQDN>:389` 后，由主机计算机账号通过 Kerberos GSS-API 取得 LDAP 服务票据；不要配置或保存 LDAP bind DN、LDAP 密码、LDAP over TLS 或 StartTLS。域管理员仅向 `<DOMAIN>\<CONNECTOR-HOST>$` 委派镜像根和隔离 OU 的必要权限。

安装时需提供 Center 地址、域标识、Connector key 和域控制器 LDAP 地址。MSI 将配置写入 `%ProgramData%\Adscope Connector\.env`，程序安装在 `%ProgramFiles%\Adscope Connector`，服务会直接启动：

```powershell
msiexec /i .\adscope-connector-v0.2.1-windows-x64.msi `
  CENTER_URL=https://center.example.com `
  DIRECTORY_DOMAIN_ID=domain-a `
  CONNECTOR_ACCESS_KEY=<connector-key> `
  DOMAIN_CONTROLLER_LDAP_URL=ldap://dc01.example.com:389
```

服务日志和同步 revision 分别保存在 `%ProgramData%\Adscope Connector\logs` 与 `%ProgramData%\Adscope Connector\adscope-connector-state.json`。升级 MSI 不覆盖 `.env`、state 或 logs。

需要修改配置时，以管理员身份执行以下命令后重启服务：

```powershell
& 'C:\Program Files\Adscope Connector\adscope-connector.exe' --configure `
  --runtime-dir "$env:ProgramData\Adscope Connector" `
  --center-url https://center.example.com `
  --directory-domain-id domain-a `
  --connector-access-key <connector-key> `
  --domain-controller-ldap-url ldap://dc01.example.com:389
Restart-Service AdscopeConnector
```

卸载 MSI 会停止并删除服务，同时保留 `.env`、state 和 logs。确认不再需要这些运行数据后，由运维人员单独清理 `%ProgramData%\Adscope Connector`。
