[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$RuntimeDir
)

$ErrorActionPreference = 'Stop'
if (Test-Path Variable:PSNativeCommandUseErrorActionPreference) {
    $PSNativeCommandUseErrorActionPreference = $false
}
$serviceName = 'ADStructureSyncConnector'
$networkServiceSid = '*S-1-5-20'

function Assert-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Run this script from an elevated PowerShell session.'
    }
}

function Invoke-Native {
    param(
        [Parameter(Mandatory)]
        [string]$Command,
        [Parameter(ValueFromRemainingArguments)]
        [string[]]$Arguments
    )

    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Command failed with exit code $LASTEXITCODE"
    }
}

function Set-ConnectorAcl {
    param(
        [Parameter(Mandatory)]
        [string]$Path,
        [Parameter(Mandatory)]
        [System.Security.AccessControl.FileSystemRights]$NetworkServiceRights,
        [switch]$InheritToChildren
    )

    $acl = Get-Acl -LiteralPath $Path
    $acl.SetAccessRuleProtection($true, $false)
    foreach ($rule in @($acl.Access)) {
        [void]$acl.RemoveAccessRuleAll($rule)
    }

    $inheritance = if ($InheritToChildren) {
        [System.Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit'
    }
    else {
        [System.Security.AccessControl.InheritanceFlags]::None
    }
    foreach ($entry in @(
        @{ Identity = 'NT AUTHORITY\SYSTEM'; Rights = [System.Security.AccessControl.FileSystemRights]::FullControl },
        @{ Identity = 'BUILTIN\Administrators'; Rights = [System.Security.AccessControl.FileSystemRights]::FullControl },
        @{ Identity = 'NT AUTHORITY\NetworkService'; Rights = $NetworkServiceRights }
    )) {
        $accessRule = [System.Security.AccessControl.FileSystemAccessRule]::new(
            $entry.Identity,
            $entry.Rights,
            $inheritance,
            [System.Security.AccessControl.PropagationFlags]::None,
            [System.Security.AccessControl.AccessControlType]::Allow
        )
        [void]$acl.AddAccessRule($accessRule)
    }
    Set-Acl -LiteralPath $Path -AclObject $acl
}

Assert-Administrator
$resolvedRuntimeDir = (Resolve-Path -LiteralPath $RuntimeDir -ErrorAction Stop).Path
if (-not [System.IO.Path]::IsPathFullyQualified($resolvedRuntimeDir)) {
    throw 'RuntimeDir must resolve to an absolute path.'
}

$executablePath = Join-Path $resolvedRuntimeDir 'adss-connector.exe'
$environmentPath = Join-Path $resolvedRuntimeDir '.env'
$statePath = Join-Path $resolvedRuntimeDir 'adss-connector-state.json'
$logsPath = Join-Path $resolvedRuntimeDir 'logs'
foreach ($path in @($executablePath, $environmentPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required Connector file does not exist: $path"
    }
}

& sc.exe query $serviceName *> $null
if ($LASTEXITCODE -eq 0) {
    throw "Service already exists: $serviceName"
}
if ($LASTEXITCODE -ne 1060) {
    throw "Unable to query service $serviceName (exit code $LASTEXITCODE)."
}

New-Item -ItemType Directory -Path $logsPath -Force | Out-Null
if (-not (Test-Path -LiteralPath $statePath -PathType Leaf)) {
    '{"applied_directory_revision":0,"applied_credential_revision":0}' |
        Set-Content -LiteralPath $statePath -Encoding utf8NoBOM
}

Invoke-Native icacls.exe $environmentPath /inheritance:r
Set-ConnectorAcl -Path $resolvedRuntimeDir -NetworkServiceRights ReadAndExecute -InheritToChildren
Set-ConnectorAcl -Path $executablePath -NetworkServiceRights ReadAndExecute
Set-ConnectorAcl -Path $environmentPath -NetworkServiceRights Read
Set-ConnectorAcl -Path $statePath -NetworkServiceRights Modify
Set-ConnectorAcl -Path $logsPath -NetworkServiceRights Modify -InheritToChildren

$binaryPath = '"{0}" --service --runtime-dir "{1}"' -f $executablePath, $resolvedRuntimeDir
$serviceCreated = $false
try {
    Invoke-Native sc.exe create $serviceName "binPath= $binaryPath" 'start= auto' `
        'obj= NT AUTHORITY\NetworkService' 'DisplayName= AD Structure Sync Connector'
    $serviceCreated = $true
    Invoke-Native sc.exe description $serviceName 'Synchronizes Center directory state to Active Directory.'
    Invoke-Native sc.exe failure $serviceName 'reset= 86400' `
        'actions= restart/5000/restart/15000/restart/60000'
    Invoke-Native sc.exe failureflag $serviceName '1'
    Invoke-Native sc.exe start $serviceName

    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        $service = Get-Service -Name $serviceName
        if ($service.Status -eq [System.ServiceProcess.ServiceControllerStatus]::Running) {
            Write-Host "Connector service is running from $resolvedRuntimeDir"
            return
        }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $deadline)

    throw "Service did not reach Running within 30 seconds: $serviceName"
}
catch {
    if ($serviceCreated) {
        & sc.exe stop $serviceName *> $null
        & sc.exe delete $serviceName *> $null
    }
    throw
}
