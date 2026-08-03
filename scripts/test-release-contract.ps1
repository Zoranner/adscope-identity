$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$buildScriptPath = Join-Path $PSScriptRoot 'build-release.ps1'
if (-not (Test-Path -LiteralPath $buildScriptPath -PathType Leaf)) {
    throw "Missing release build script: $buildScriptPath"
}

. $buildScriptPath

$temporaryRoot = Join-Path ([System.IO.Path]::GetTempPath()) "adss-release-contract-$PID-$([Guid]::NewGuid().ToString('N'))"
$assemblyRoot = Join-Path $temporaryRoot 'assembly'
$fakeRepository = Join-Path $temporaryRoot 'dirty-repository'
New-Item -ItemType Directory -Path $assemblyRoot, $fakeRepository -Force | Out-Null

try {
    $actualVersion = Get-AdssVersion -RepositoryRoot $repositoryRoot
    if ($actualVersion -ne '0.1.0') {
        throw "Unexpected workspace version: $actualVersion"
    }

    $wrongVersionRejected = $false
    try {
        Assert-ReleaseVersion -RequestedVersion '9.9.9' -ActualVersion $actualVersion
    }
    catch {
        $wrongVersionRejected = $_.Exception.Message -match 'does not match'
    }
    if (-not $wrongVersionRejected) {
        throw 'Mismatched release version was not rejected.'
    }

    $fakeBinary = Join-Path $assemblyRoot 'fake-adss-connector.exe'
    Set-Content -LiteralPath $fakeBinary -Value 'fake connector binary' -NoNewline
    $connectorArchive = Join-Path $assemblyRoot 'adss-connector-v0.1.0-windows-x86_64.zip'
    New-ConnectorArchive `
        -RepositoryRoot $repositoryRoot `
        -ConnectorBinary $fakeBinary `
        -OutputPath $connectorArchive

    $expanded = Join-Path $assemblyRoot 'expanded'
    Expand-Archive -LiteralPath $connectorArchive -DestinationPath $expanded
    foreach ($entry in @(
        'adss-connector.exe',
        '.env.example',
        'install-service.ps1',
        'uninstall-service.ps1',
        'README.md'
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $expanded $entry) -PathType Leaf)) {
            throw "Connector archive is missing $entry"
        }
    }

    $centerArchive = Join-Path $assemblyRoot 'adss-center-v0.1.0-linux-amd64.tar'
    Set-Content -LiteralPath $centerArchive -Value 'fake center image archive' -NoNewline
    $manifestPath = Join-Path $assemblyRoot 'manifest.json'
    $checksumsPath = Join-Path $assemblyRoot 'SHA256SUMS'
    Write-ReleaseManifest `
        -Version '0.1.0' `
        -Revision '0123456789abcdef' `
        -Target 'windows-x86_64,linux-amd64' `
        -Artifacts @($connectorArchive, $centerArchive) `
        -ManifestPath $manifestPath `
        -ChecksumsPath $checksumsPath

    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    if ($manifest.version -ne '0.1.0' -or $manifest.revision -ne '0123456789abcdef') {
        throw 'Release manifest version or revision is incorrect.'
    }
    if ($manifest.target -ne 'windows-x86_64,linux-amd64') {
        throw 'Release manifest target is incorrect.'
    }
    foreach ($artifact in @($connectorArchive, $centerArchive)) {
        $name = Split-Path -Leaf $artifact
        $expectedHash = (Get-FileHash -LiteralPath $artifact -Algorithm SHA256).Hash.ToLowerInvariant()
        $manifestArtifact = $manifest.artifacts | Where-Object name -EQ $name
        if ($manifestArtifact.sha256 -ne $expectedHash) {
            throw "Manifest hash is incorrect for $name"
        }
        $checksumLine = "$expectedHash  $name"
        if ((Get-Content -LiteralPath $checksumsPath) -notcontains $checksumLine) {
            throw "SHA256SUMS is incorrect for $name"
        }
    }

    git -C $fakeRepository init --quiet
    git -C $fakeRepository config user.email 'release-contract@example.invalid'
    git -C $fakeRepository config user.name 'Release Contract'
    Set-Content -LiteralPath (Join-Path $fakeRepository 'tracked.txt') -Value 'tracked'
    git -C $fakeRepository add tracked.txt
    git -C $fakeRepository commit --quiet -m 'initial'
    Assert-CleanWorktree -RepositoryRoot $fakeRepository
    Set-Content -LiteralPath (Join-Path $fakeRepository 'untracked.txt') -Value 'dirty'
    $dirtyRejected = $false
    try {
        Assert-CleanWorktree -RepositoryRoot $fakeRepository
    }
    catch {
        $dirtyRejected = $_.Exception.Message -match 'clean Git worktree'
    }
    if (-not $dirtyRejected) {
        throw 'Dirty worktree was not rejected.'
    }
}
finally {
    Remove-Item -LiteralPath $temporaryRoot -Recurse -Force
}

Write-Host 'Release assembly contract passed.'
