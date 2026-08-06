$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$dockerfilePath = Join-Path $repositoryRoot 'Dockerfile'
$dockerignorePath = Join-Path $repositoryRoot '.dockerignore'
$composePath = Join-Path $repositoryRoot 'deploy\center\compose.yaml'
$environmentExamplePath = Join-Path $repositoryRoot 'deploy\center\center.env.example'
$centerEnvironmentExamplePath = Join-Path $repositoryRoot 'center\.env.example'

$requiredFiles = @(
    $dockerfilePath,
    $dockerignorePath,
    $composePath,
    $environmentExamplePath,
    $centerEnvironmentExamplePath
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
Assert-Contains $compose '(?m)^\s+-\s+\./oidc-private-key\.pem:/run/secrets/oidc-private-key\.pem:ro\s*$' 'read-only OIDC private key mount'
if ($compose -match '(?m)^\s+ports:$') {
    throw 'Center Compose must not publish a host port'
}
if ($compose -match '(?i)certificate|/etc/(?:ssl|letsencrypt)|(?:tls|ssl)[_-]?(?:cert|key)') {
    throw 'Center Compose must not mount or manage TLS certificates'
}

$environmentExample = Get-Content -LiteralPath $environmentExamplePath -Raw
$centerEnvironmentExample = Get-Content -LiteralPath $centerEnvironmentExamplePath -Raw
foreach ($name in @(
    'ADSS_PASSWORD_ENCRYPTION_KEY',
    'ADSS_PASSWORD_HASH_PROVIDER',
    'ADSS_USER_SESSION_KEY',
    'ADSS_MANAGEMENT_TOKEN',
    'ADSS_OIDC_ISSUER',
    'ADSS_OIDC_PRIVATE_KEY_FILE',
    'ADSS_OIDC_ALLOW_INSECURE_WEB_LOOPBACK_REDIRECTS'
)) {
    Assert-Contains $environmentExample "(?m)^$name=" "environment variable $name"
    Assert-Contains $centerEnvironmentExample "(?m)^$name=" "Center environment variable $name"
}

Assert-Contains $environmentExample '(?m)^ADSS_OIDC_ISSUER=https://center\.example\.com$' 'production HTTPS OIDC issuer example'
Assert-Contains $environmentExample '(?m)^ADSS_OIDC_PRIVATE_KEY_FILE=/run/secrets/oidc-private-key\.pem$' 'container OIDC private key path'
Assert-Contains $environmentExample '(?m)^ADSS_OIDC_ALLOW_INSECURE_WEB_LOOPBACK_REDIRECTS=false$' 'secure OIDC loopback redirect default'
Assert-Contains $centerEnvironmentExample '(?m)^ADSS_OIDC_ALLOW_INSECURE_WEB_LOOPBACK_REDIRECTS=false$' 'secure Center OIDC loopback redirect default'

foreach ($deliveryFilePath in @(
    $dockerfilePath,
    $composePath,
    $environmentExamplePath,
    $centerEnvironmentExamplePath
)) {
    $deliveryFile = Get-Content -LiteralPath $deliveryFilePath -Raw
    if ($deliveryFile -match '(?m)^-----BEGIN (?:[A-Z0-9]+ )*PRIVATE KEY-----\s*$') {
        throw "Docker delivery file must not contain a PEM private key: $deliveryFilePath"
    }
}

Write-Host 'Docker delivery contract passed.'
