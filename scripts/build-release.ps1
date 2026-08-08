[CmdletBinding()]
param(
    [string]$Version,
    [switch]$SkipDocker,
    [string]$OutputRoot = 'dist'
)

$ErrorActionPreference = 'Stop'
if (Test-Path Variable:PSNativeCommandUseErrorActionPreference) {
    $PSNativeCommandUseErrorActionPreference = $false
}

function Assert-NativeSuccess {
    param([Parameter(Mandatory)][string]$Description)

    if ($LASTEXITCODE -ne 0) {
        throw "$Description failed with exit code $LASTEXITCODE"
    }
}

function Assert-CleanWorktree {
    param([Parameter(Mandatory)][string]$RepositoryRoot)

    $status = git -C $RepositoryRoot status --porcelain=v1 --untracked-files=all
    Assert-NativeSuccess 'git status'
    if ($status) {
        throw 'Release builds require a clean Git worktree.'
    }
}

function Get-AdscopeVersion {
    param([Parameter(Mandatory)][string]$RepositoryRoot)

    $manifestPath = Join-Path $RepositoryRoot 'Cargo.toml'
    $metadataJson = cargo metadata --no-deps --format-version 1 --manifest-path $manifestPath
    Assert-NativeSuccess 'cargo metadata'
    $metadata = $metadataJson | ConvertFrom-Json
    $releasePackages = @(
        $metadata.packages | Where-Object { $_.name -in @('adscope-center', 'adscope-connector') }
    )
    if ($releasePackages.Count -ne 2) {
        throw 'Cargo metadata must contain both adscope-center and adscope-connector.'
    }
    $versions = @(
        $releasePackages |
            ForEach-Object version |
            Sort-Object -Unique
    )
    if ($versions.Count -ne 1) {
        throw 'Center and Connector must have the same package version.'
    }
    return [string]$versions[0]
}

function Assert-ReleaseVersion {
    param(
        [Parameter(Mandatory)][string]$RequestedVersion,
        [Parameter(Mandatory)][string]$ActualVersion
    )

    if ($RequestedVersion -ne $ActualVersion) {
        throw "Requested version $RequestedVersion does not match workspace version $ActualVersion."
    }
}

function New-ConnectorArchive {
    param(
        [Parameter(Mandatory)][string]$RepositoryRoot,
        [Parameter(Mandatory)][string]$ConnectorBinary,
        [Parameter(Mandatory)][string]$OutputPath
    )

    if (-not (Test-Path -LiteralPath $ConnectorBinary -PathType Leaf)) {
        throw "Connector binary does not exist: $ConnectorBinary"
    }
    $staging = Join-Path ([System.IO.Path]::GetTempPath()) "adscope-connector-archive-$PID-$([Guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Path $staging -Force | Out-Null
    try {
        $entries = @{
            'adscope-connector.exe' = $ConnectorBinary
            '.env.example' = (Join-Path $RepositoryRoot 'connector\.env.example')
            'install-service.ps1' = (Join-Path $RepositoryRoot 'deploy\connector\install-service.ps1')
            'uninstall-service.ps1' = (Join-Path $RepositoryRoot 'deploy\connector\uninstall-service.ps1')
            'README.md' = (Join-Path $RepositoryRoot 'deploy\connector\README.md')
        }
        foreach ($entry in $entries.GetEnumerator()) {
            if (-not (Test-Path -LiteralPath $entry.Value -PathType Leaf)) {
                throw "Connector package source does not exist: $($entry.Value)"
            }
            Copy-Item -LiteralPath $entry.Value -Destination (Join-Path $staging $entry.Key)
        }

        $outputDirectory = Split-Path -Parent $OutputPath
        New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
        if (Test-Path -LiteralPath $OutputPath) {
            Remove-Item -LiteralPath $OutputPath -Force
        }
        $archiveEntries = Get-ChildItem -LiteralPath $staging -Force | ForEach-Object FullName
        Compress-Archive -LiteralPath $archiveEntries -DestinationPath $OutputPath -CompressionLevel Optimal
    }
    finally {
        Remove-Item -LiteralPath $staging -Recurse -Force
    }
}

function Write-ReleaseManifest {
    param(
        [Parameter(Mandatory)][string]$Version,
        [Parameter(Mandatory)][string]$Revision,
        [Parameter(Mandatory)][string]$Target,
        [Parameter(Mandatory)][string[]]$Artifacts,
        [Parameter(Mandatory)][string]$ManifestPath,
        [Parameter(Mandatory)][string]$ChecksumsPath
    )

    $artifactRecords = @(
        $Artifacts |
            Sort-Object { Split-Path -Leaf $_ } |
            ForEach-Object {
                if (-not (Test-Path -LiteralPath $_ -PathType Leaf)) {
                    throw "Release artifact does not exist: $_"
                }
                [ordered]@{
                    name = Split-Path -Leaf $_
                    sha256 = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()
                }
            }
    )
    $manifest = [ordered]@{
        version = $Version
        revision = $Revision
        target = $Target
        artifacts = $artifactRecords
    }
    $manifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $ManifestPath -Encoding utf8NoBOM
    $artifactRecords |
        ForEach-Object { "$($_.sha256)  $($_.name)" } |
        Set-Content -LiteralPath $ChecksumsPath -Encoding ascii
}

function Invoke-ReleaseBuild {
    param(
        [Parameter(Mandatory)][string]$RepositoryRoot,
        [string]$RequestedVersion,
        [switch]$SkipDockerBuild,
        [Parameter(Mandatory)][string]$ReleaseOutputRoot
    )

    $actualVersion = Get-AdscopeVersion -RepositoryRoot $RepositoryRoot
    if ([string]::IsNullOrWhiteSpace($RequestedVersion)) {
        $RequestedVersion = $actualVersion
    }
    Assert-ReleaseVersion -RequestedVersion $RequestedVersion -ActualVersion $actualVersion
    Assert-CleanWorktree -RepositoryRoot $RepositoryRoot

    $revision = (git -C $RepositoryRoot rev-parse HEAD).Trim()
    Assert-NativeSuccess 'git rev-parse HEAD'
    $resolvedOutputRoot = if ([System.IO.Path]::IsPathFullyQualified($ReleaseOutputRoot)) {
        [System.IO.Path]::GetFullPath($ReleaseOutputRoot)
    }
    else {
        [System.IO.Path]::GetFullPath((Join-Path $RepositoryRoot $ReleaseOutputRoot))
    }
    $releaseDirectory = Join-Path $resolvedOutputRoot "v$RequestedVersion"
    New-Item -ItemType Directory -Path $releaseDirectory -Force | Out-Null
    foreach ($name in @(
        "adscope-connector-v$RequestedVersion-windows-x86_64.zip",
        "adscope-center-v$RequestedVersion-linux-amd64.tar",
        'manifest.json',
        'SHA256SUMS'
    )) {
        $existingOutput = Join-Path $releaseDirectory $name
        if (Test-Path -LiteralPath $existingOutput -PathType Leaf) {
            Remove-Item -LiteralPath $existingOutput -Force
        }
    }

    Push-Location $RepositoryRoot
    try {
        cargo build --release --locked -p adscope-connector
        Assert-NativeSuccess 'Connector release build'
        $connectorArchive = Join-Path $releaseDirectory "adscope-connector-v$RequestedVersion-windows-x86_64.zip"
        New-ConnectorArchive `
            -RepositoryRoot $RepositoryRoot `
            -ConnectorBinary (Join-Path $RepositoryRoot 'target\release\adscope-connector.exe') `
            -OutputPath $connectorArchive

        $artifacts = @($connectorArchive)
        $target = 'windows-x86_64'
        if (-not $SkipDockerBuild) {
            $imageTag = "adscope-center:$RequestedVersion"
            $centerArchive = Join-Path $releaseDirectory "adscope-center-v$RequestedVersion-linux-amd64.tar"
            docker build --platform linux/amd64 `
                --build-arg "VERSION=$RequestedVersion" `
                --build-arg "REVISION=$revision" `
                --tag $imageTag .
            Assert-NativeSuccess 'Center Docker image build'
            docker save --output $centerArchive $imageTag
            Assert-NativeSuccess 'Center Docker image export'
            $artifacts += $centerArchive
            $target = 'windows-x86_64,linux-amd64'
        }

        Write-ReleaseManifest `
            -Version $RequestedVersion `
            -Revision $revision `
            -Target $target `
            -Artifacts $artifacts `
            -ManifestPath (Join-Path $releaseDirectory 'manifest.json') `
            -ChecksumsPath (Join-Path $releaseDirectory 'SHA256SUMS')
    }
    finally {
        Pop-Location
    }

    Write-Host "Release artifacts written to $releaseDirectory"
}

if ($MyInvocation.InvocationName -ne '.') {
    $repositoryRoot = Split-Path -Parent $PSScriptRoot
    Invoke-ReleaseBuild `
        -RepositoryRoot $repositoryRoot `
        -RequestedVersion $Version `
        -SkipDockerBuild:$SkipDocker `
        -ReleaseOutputRoot $OutputRoot
}
