$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$dockerfilePath = Join-Path $repositoryRoot 'Dockerfile'
$dockerignorePath = Join-Path $repositoryRoot '.dockerignore'
$composePath = Join-Path $repositoryRoot 'deploy\center\compose.yaml'
$environmentExamplePath = Join-Path $repositoryRoot 'deploy\center\center.env.example'

$requiredFiles = @(
    $dockerfilePath,
    $dockerignorePath,
    $composePath,
    $environmentExamplePath
)
$missingFiles = $requiredFiles | Where-Object { -not (Test-Path -LiteralPath $_ -PathType Leaf) }
if ($missingFiles.Count -gt 0) {
    throw "Missing Docker delivery files: $($missingFiles -join ', ')"
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
        throw "Missing Docker contract: $Description"
    }
}

$dockerfile = Get-Content -LiteralPath $dockerfilePath -Raw
Assert-Contains $dockerfile 'bun install --frozen-lockfile' 'frozen Bun install'
Assert-Contains $dockerfile 'cargo build --release --locked -p adss-center' 'locked Center build'
Assert-Contains $dockerfile 'USER 10001:10001' 'non-root runtime user'
Assert-Contains $dockerfile '/api/health' 'container health check'
Assert-Contains $dockerfile 'VOLUME \["/data"\]' 'SQLite data volume'
Assert-Contains $dockerfile 'ADSS_WEB_ROOT=/app/web' 'bundled Web root'
Assert-Contains $dockerfile 'ADSS_DATABASE_URL=sqlite:///data/adss\.db\?mode=rwc' 'SQLite database URL'

$dockerignore = Get-Content -LiteralPath $dockerignorePath -Raw
Assert-Contains $dockerignore '(?m)^\.env$' 'secret environment files excluded'
Assert-Contains $dockerignore '(?m)^target/$' 'Rust target excluded'
Assert-Contains $dockerignore '(?m)^node_modules/$' 'Node modules excluded'
Assert-Contains $dockerignore '(?m)^\.output/$' 'Nuxt output excluded'

$compose = Get-Content -LiteralPath $composePath -Raw
Assert-Contains $compose '(?m)^\s+expose:$' 'internal-only port exposure'
Assert-Contains $compose '(?m)^\s+read_only:\s+true$' 'read-only root filesystem'
Assert-Contains $compose 'adss-center-data:/data' 'persistent SQLite volume'
if ($compose -match '(?m)^\s+ports:$') {
    throw 'Center Compose must not publish a host port'
}
if ($compose -match '(?i)certificate|/etc/ssl|\.pem') {
    throw 'Center Compose must not mount or manage TLS certificates'
}

$environmentExample = Get-Content -LiteralPath $environmentExamplePath -Raw
foreach ($name in @(
    'ADSS_PASSWORD_ENCRYPTION_KEY',
    'ADSS_PASSWORD_HASH_PROVIDER',
    'ADSS_USER_SESSION_KEY',
    'ADSS_MANAGEMENT_TOKEN'
)) {
    Assert-Contains $environmentExample "(?m)^$name=" "environment variable $name"
}

Write-Host 'Docker delivery contract passed.'
