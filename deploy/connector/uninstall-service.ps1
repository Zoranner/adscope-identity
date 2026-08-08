[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
if (Test-Path Variable:PSNativeCommandUseErrorActionPreference) {
    $PSNativeCommandUseErrorActionPreference = $false
}
$serviceName = 'AdscopeConnector'

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
& sc.exe query $serviceName *> $null
if ($LASTEXITCODE -eq 1060) {
    Write-Host "Connector service is not installed: $serviceName"
    return
}
if ($LASTEXITCODE -ne 0) {
    throw "Unable to query service $serviceName (exit code $LASTEXITCODE)."
}

$service = Get-Service -Name $serviceName
if ($service.Status -ne [System.ServiceProcess.ServiceControllerStatus]::Stopped) {
    Invoke-Native sc.exe stop $serviceName
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        Start-Sleep -Milliseconds 250
        $service.Refresh()
    } while (
        $service.Status -ne [System.ServiceProcess.ServiceControllerStatus]::Stopped -and
        [DateTime]::UtcNow -lt $deadline
    )
    if ($service.Status -ne [System.ServiceProcess.ServiceControllerStatus]::Stopped) {
        throw "Service did not stop within 30 seconds: $serviceName"
    }
}

Invoke-Native sc.exe delete $serviceName
Write-Host 'Connector service removed. The .env, state file, and logs were preserved.'
