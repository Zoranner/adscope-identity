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
$localServiceSid = '*S-1-5-19'

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

Invoke-Native icacls.exe $resolvedRuntimeDir /grant "${localServiceSid}:(RX)"
Invoke-Native icacls.exe $executablePath /grant "${localServiceSid}:(R)"
Invoke-Native icacls.exe $environmentPath /grant "${localServiceSid}:(R)"
Invoke-Native icacls.exe $statePath /grant "${localServiceSid}:(M)"
Invoke-Native icacls.exe $logsPath /grant "${localServiceSid}:(OI)(CI)(M)"

$binaryPath = '"{0}" --service --runtime-dir "{1}"' -f $executablePath, $resolvedRuntimeDir
$serviceCreated = $false
try {
    Invoke-Native sc.exe create $serviceName "binPath= $binaryPath" 'start= auto' `
        'obj= NT AUTHORITY\LocalService' 'DisplayName= AD Structure Sync Connector'
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
