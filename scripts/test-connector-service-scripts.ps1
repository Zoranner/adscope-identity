$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$installPath = Join-Path $repositoryRoot 'deploy\connector\install-service.ps1'
$uninstallPath = Join-Path $repositoryRoot 'deploy\connector\uninstall-service.ps1'

foreach ($path in @($installPath, $uninstallPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Missing Connector service script: $path"
    }

    $tokens = $null
    $errors = $null
    [System.Management.Automation.Language.Parser]::ParseFile($path, [ref]$tokens, [ref]$errors) | Out-Null
    if ($errors.Count -gt 0) {
        throw "PowerShell parse errors in ${path}: $($errors.Message -join '; ')"
    }
}

function Assert-Contains {
    param(
        [Parameter(Mandatory)]
        [string]$Content,
        [Parameter(Mandatory)]
        [string]$Pattern,
        [Parameter(Mandatory)]
        [string]$Description
    )

    if ($Content -notmatch $Pattern) {
        throw "Missing Connector service script contract: $Description"
    }
}

$install = Get-Content -LiteralPath $installPath -Raw
Assert-Contains $install 'ADStructureSyncConnector' 'fixed service name'
Assert-Contains $install 'S-1-5-19|LocalService' 'LocalService identity'
Assert-Contains $install '--service' 'service process switch'
Assert-Contains $install '--runtime-dir' 'explicit runtime directory'
Assert-Contains $install '(?i)start=.{0,8}auto|StartupType.{0,8}Automatic' 'automatic startup'
Assert-Contains $install '(?i)failure' 'failure recovery configuration'
Assert-Contains $install '(?i)administrator|WindowsPrincipal|IsInRole' 'administrator check'

$uninstall = Get-Content -LiteralPath $uninstallPath -Raw
Assert-Contains $uninstall 'ADStructureSyncConnector' 'fixed uninstall service name'
Assert-Contains $uninstall '(?i)stop' 'service stop'
Assert-Contains $uninstall '(?i)delete' 'service deletion'
if ($uninstall -match '(?i)Remove-Item.+(?:\.env|state|logs)') {
    throw 'Uninstall script must preserve .env, state, and logs'
}

Write-Host 'Connector service script contract passed.'
