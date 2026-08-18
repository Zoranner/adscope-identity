$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$dockerfilePath = Join-Path $repositoryRoot 'Dockerfile'
$dockerignorePath = Join-Path $repositoryRoot '.dockerignore'
$composePath = Join-Path $repositoryRoot 'deploy\center\docker-compose.yml'
$environmentExamplePath = Join-Path $repositoryRoot 'deploy\center\.env.example'
$centerEnvironmentExamplePath = Join-Path $repositoryRoot 'center\.env.example'
$releaseWorkflowPath = Join-Path $repositoryRoot '.github\workflows\release.yml'
$deploymentGuidePath = Join-Path $repositoryRoot 'docs\guide\deployment.md'

$requiredFiles = @(
    $dockerfilePath,
    $dockerignorePath,
    $composePath,
    $environmentExamplePath,
    $centerEnvironmentExamplePath,
    $releaseWorkflowPath,
    $deploymentGuidePath
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
Assert-Contains $dockerfile '(?m)^RUN bun install$' 'Bun install without an internal registry override'
if ($dockerfile -match 'center/web/bun\.lock') {
    throw 'Release image must not copy the internal Bun lockfile'
}
Assert-Contains $dockerfile 'cargo build --release --locked -p adscope-center' 'locked Center build'
Assert-Contains $dockerfile 'USER 10001:10001' 'non-root runtime user'
Assert-Contains $dockerfile '/api/health' 'container health check'
Assert-Contains $dockerfile 'VOLUME \["/data"\]' 'SQLite data volume'
Assert-Contains $dockerfile 'WEB_ROOT=/app/web' 'bundled Web root'
Assert-Contains $dockerfile 'DATABASE_URL=sqlite:///data/adscope\.db\?mode=rwc' 'SQLite database URL'

$dockerignore = Get-Content -LiteralPath $dockerignorePath -Raw
Assert-Contains $dockerignore '(?m)^\.env$' 'secret environment files excluded'
Assert-Contains $dockerignore '(?m)^target/$' 'Rust target excluded'
Assert-Contains $dockerignore '(?m)^node_modules/$' 'Node modules excluded'
Assert-Contains $dockerignore '(?m)^\.output/$' 'Nuxt output excluded'

$compose = Get-Content -LiteralPath $composePath -Raw
Assert-Contains $compose 'ghcr\.io/zoranner/adscope-center:0\.1\.0' 'published Center image name'
Assert-Contains $compose '(?m)^\s+env_file:$' 'Center environment file declaration'
Assert-Contains $compose '(?m)^\s+-\s+\.env\s*$' 'Center .env file'
Assert-Contains $compose '(?m)^\s+-\s+\./data:/data\s*$' 'persistent SQLite bind mount'
Assert-Contains $compose '(?m)^\s+-\s+\./app/secrets:/run/secrets:ro\s*$' 'read-only OIDC secret directory mount'
Assert-Contains $compose '(?m)^\s+ports:$' 'published Center port'
Assert-Contains $compose '(?m)^\s+-\s+["'']8080:8080["'']\s*$' 'Center host port mapping'
Assert-Contains $compose '(?m)^\s+default:$' 'default Docker network'
Assert-Contains $compose '(?m)^\s+external:\s+true$' 'external Docker network'
Assert-Contains $compose '(?m)^\s+name:\s+adscope\s*$' 'adscope Docker network name'
if ($compose -match '(?m)^\s+expose:$') {
    throw 'Center Compose must publish its host port instead of using expose'
}
if ($compose -match '(?m)^\s+read_only:\s+true$') {
    throw 'Center Compose must not use a read-only root filesystem'
}
if ($compose -match '(?m)^\s+tmpfs:$') {
    throw 'Center Compose must not define tmpfs mounts'
}
if ($compose -match '(?m)^volumes:$') {
    throw 'Center Compose must use bind mounts, not named volumes'
}
if ($compose -match '(?m)^\s*ADSCOPE_OIDC_PRIVATE_KEY_FILE=') {
    throw 'Center Compose must not configure the fixed OIDC private key path'
}
if ($compose -match '(?i)certificate|/etc/(?:ssl|letsencrypt)|(?:tls|ssl)[_-]?(?:cert|key)') {
    throw 'Center Compose must not mount or manage TLS certificates'
}

$releaseWorkflow = Get-Content -LiteralPath $releaseWorkflowPath -Raw
Assert-Contains $releaseWorkflow '(?m)^\s+packages:\s+write$' 'GitHub Packages publish permission'
Assert-Contains $releaseWorkflow 'ghcr\.io/zoranner/adscope-center' 'GitHub Container Registry image name'
Assert-Contains $releaseWorkflow 'docker login ghcr\.io' 'GitHub Container Registry login'
Assert-Contains $releaseWorkflow 'docker push "\$IMAGE:\$VERSION"' 'versioned Center image push'

$deploymentGuide = Get-Content -LiteralPath $deploymentGuidePath -Raw
Assert-Contains $deploymentGuide 'ghcr\.io/zoranner/adscope-center:0\.1\.0' 'published Center image deployment reference'
Assert-Contains $deploymentGuide 'docker login ghcr\.io' 'private GitHub Container Registry login instruction'

$environmentExample = Get-Content -LiteralPath $environmentExamplePath -Raw
$centerEnvironmentExample = Get-Content -LiteralPath $centerEnvironmentExamplePath -Raw
foreach ($name in @(
    'DATABASE_URL',
    'PASSWORD_ENCRYPTION_KEY',
    'PASSWORD_HASH_PROVIDER',
    'SESSION_KEY',
    'MANAGEMENT_TOKEN',
    'OIDC_ISSUER',
    'OIDC_LOOPBACK_HTTP'
)) {
    Assert-Contains $environmentExample "(?m)^$name=" "environment variable $name"
    Assert-Contains $centerEnvironmentExample "(?m)^$name=" "Center environment variable $name"
}

Assert-Contains $environmentExample '(?m)^OIDC_ISSUER=https://center\.example\.com$' 'production HTTPS OIDC issuer example'
Assert-Contains $environmentExample '(?m)^OIDC_LOOPBACK_HTTP=false$' 'secure OIDC loopback redirect default'
Assert-Contains $centerEnvironmentExample '(?m)^OIDC_LOOPBACK_HTTP=false$' 'secure Center OIDC loopback redirect default'

foreach ($environmentFile in @($environmentExample, $centerEnvironmentExample)) {
    if ($environmentFile -match '(?m)^ADSCOPE_') {
        throw 'Center environment variables must use their category name without an ADSCOPE prefix'
    }
    if ($environmentFile -match '(?m)^ADSCOPE_OIDC_PRIVATE_KEY_FILE=') {
        throw 'OIDC private key path must be fixed by the application, not configured through the environment'
    }
}

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
