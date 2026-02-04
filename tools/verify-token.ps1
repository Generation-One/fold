# Verify that a token exists in the database and check its configuration
param(
    [string]$Token,
    [string]$DbPath = "d:\hh\git\g1\fold\srv\data\fold.db"
)

if (-not $Token) {
    Write-Host "Usage: .\verify-token.ps1 -Token 'fold_xxxxx...'" -ForegroundColor Yellow
    exit 1
}

# Extract prefix (first 8 chars after "fold_")
if (-not $Token.StartsWith("fold_")) {
    Write-Host "Error: Token must start with 'fold_'" -ForegroundColor Red
    exit 1
}

$tokenBody = $Token.Substring(5)
if ($tokenBody.Length -lt 9) {
    Write-Host "Error: Token is too short (must have at least 8 char prefix + 1 char secret)" -ForegroundColor Red
    exit 1
}

$prefix = $tokenBody.Substring(0, 8)

Write-Host "Token Format Check:" -ForegroundColor Cyan
Write-Host "  Full Token: $Token" -ForegroundColor Gray
Write-Host "  Prefix: $prefix" -ForegroundColor Gray

# Hash the token
$sha256 = [System.Security.Cryptography.SHA256]::Create()
$hashBytes = $sha256.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($Token))
$tokenHash = [BitConverter]::ToString($hashBytes) -replace '-',''
$tokenHash = $tokenHash.ToLower()

Write-Host "  Hash: $tokenHash" -ForegroundColor Gray
Write-Host ""

# Query database for token
$query = @"
SELECT
    t.id as token_id,
    t.user_id,
    t.name,
    t.token_prefix,
    t.token_hash,
    t.project_ids,
    t.expires_at,
    t.revoked_at,
    u.email,
    u.role,
    u.provider
FROM api_tokens t
JOIN users u ON t.user_id = u.id
WHERE t.token_prefix = ?
"@

Write-Host "Querying database for token with prefix: $prefix" -ForegroundColor Cyan

$result = sqlite3 -json $DbPath $query $prefix | ConvertFrom-Json

if (-not $result) {
    Write-Host "Error: Token prefix not found in database" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "Token Found in Database:" -ForegroundColor Green
Write-Host "  Token ID: $($result.token_id)" -ForegroundColor Gray
Write-Host "  User ID: $($result.user_id)" -ForegroundColor Gray
Write-Host "  User Email: $($result.email)" -ForegroundColor Gray
Write-Host "  User Role: $($result.role)" -ForegroundColor Gray
Write-Host "  User Provider: $($result.provider)" -ForegroundColor Gray
Write-Host "  Token Name: $($result.name)" -ForegroundColor Gray
Write-Host "  Project IDs: $($result.project_ids)" -ForegroundColor Gray
Write-Host ""

Write-Host "Hash Verification:" -ForegroundColor Cyan
if ($tokenHash -eq $result.token_hash) {
    Write-Host "  ✓ Token hash MATCHES database" -ForegroundColor Green
} else {
    Write-Host "  ✗ Token hash DOES NOT MATCH database" -ForegroundColor Red
    Write-Host "    Expected: $($result.token_hash)" -ForegroundColor Red
    Write-Host "    Got: $tokenHash" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "Token Status:" -ForegroundColor Cyan
if ($result.revoked_at) {
    Write-Host "  ✗ Token is REVOKED" -ForegroundColor Red
    exit 1
} else {
    Write-Host "  ✓ Token is NOT revoked" -ForegroundColor Green
}

if ($result.expires_at) {
    Write-Host "  Expires at: $($result.expires_at)" -ForegroundColor Gray
} else {
    Write-Host "  ✓ Token does not expire" -ForegroundColor Green
}

Write-Host ""
Write-Host "Admin Status:" -ForegroundColor Cyan
if ($result.role -eq "admin") {
    Write-Host "  ✓ User has ADMIN role" -ForegroundColor Green
} else {
    Write-Host "  ✗ User has role: $($result.role)" -ForegroundColor Yellow
    exit 1
}

Write-Host ""
Write-Host "✓ Token is valid and ready to use!" -ForegroundColor Green
