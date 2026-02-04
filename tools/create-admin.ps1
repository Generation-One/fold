# Create admin user and API token for Fold
# Reads configuration from .env file in the srv root directory

param(
    [string]$AdminEmail = "admin@fold.local",
    [string]$DisplayName = "Admin User",
    [string]$EnvPath = (Join-Path (Split-Path -Parent $PSScriptRoot) ".env")
)

function Load-EnvFile {
    param([string]$Path)

    if (-not (Test-Path $Path)) {
        Write-Host "Warning: .env file not found at $Path" -ForegroundColor Yellow
        return @{}
    }

    $env_vars = @{}
    Get-Content $Path | ForEach-Object {
        $line = $_.Trim()
        if ($line -and -not $line.StartsWith("#")) {
            $key, $value = $line -split '=', 2
            if ($key -and $value) {
                $env_vars[$key.Trim()] = $value.Trim()
            }
        }
    }
    return $env_vars
}

# Load environment variables
$envVars = Load-EnvFile -Path $EnvPath

# Determine database path
$dbPath = $envVars['DATABASE_PATH']
if (-not $dbPath) {
    $dbPath = "./data/fold.db"
}

# Resolve relative paths relative to srv root
if (-not [System.IO.Path]::IsPathRooted($dbPath)) {
    $srvRoot = Split-Path -Parent $PSScriptRoot
    $dbPath = Join-Path $srvRoot $dbPath
}

Write-Host "Using database: $dbPath" -ForegroundColor Gray

# Check if database exists
if (-not (Test-Path $dbPath)) {
    Write-Host "Error: Database not found at $dbPath" -ForegroundColor Red
    exit 1
}

# Generate UUIDs for user and token
$userId = [guid]::NewGuid().ToString()
$tokenId = [guid]::NewGuid().ToString()

# Generate token in format: fold_{8-char-prefix}_{32-char-secret}
# Using alphanumeric characters from base62 encoding

function Generate-RandomAlphanumeric {
    param([int]$Length)
    $chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"
    $random = New-Object System.Random
    $result = ""
    for ($i = 0; $i -lt $Length; $i++) {
        $result += $chars[$random.Next($chars.Length)]
    }
    return $result
}

$prefix = Generate-RandomAlphanumeric -Length 8
$secret = Generate-RandomAlphanumeric -Length 32
$token = "fold_" + $prefix + "_" + $secret

# Hash the FULL token (SHA256)
$sha256 = [System.Security.Cryptography.SHA256]::Create()
$hashBytes = $sha256.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($token))
$hash = [BitConverter]::ToString($hashBytes) -replace '-',''
$hash = $hash.ToLower()

# Create admin user (role='admin')
$createUserSql = @"
INSERT INTO users (id, provider, subject, email, display_name, role, created_at, updated_at)
VALUES ('$userId', 'local', 'admin', '$AdminEmail', '$DisplayName', 'admin', datetime('now'), datetime('now'));
"@

# Create API token for admin user
$createTokenSql = @"
INSERT INTO api_tokens (id, user_id, name, token_hash, token_prefix, project_ids, created_at)
VALUES ('$tokenId', '$userId', 'Admin Token', '$hash', '$prefix', '[]', datetime('now'));
"@

try {
    sqlite3 $dbPath $createUserSql
    Write-Host "Admin user created successfully!" -ForegroundColor Green

    sqlite3 $dbPath $createTokenSql
    Write-Host "API token created successfully!" -ForegroundColor Green
    Write-Host ""
    Write-Host "FOLD_TOKEN=$token" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "User ID: $userId" -ForegroundColor Yellow
    Write-Host "Token ID: $tokenId" -ForegroundColor Yellow
    Write-Host "Email: $AdminEmail" -ForegroundColor Yellow
    Write-Host ""
    Write-Host "Set this in your environment:" -ForegroundColor Yellow
    Write-Host "`$env:FOLD_TOKEN = `"$token`""
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
    exit 1
}
