# Check if Fold server is running and healthy
$response = try { Invoke-RestMethod -Uri "http://localhost:8765/health" -TimeoutSec 2 } catch { $null }

if ($response) {
    Write-Host "Fold server is RUNNING" -ForegroundColor Green
    Write-Host ($response | ConvertTo-Json -Compress)
    exit 0
} else {
    Write-Host "Fold server is NOT running" -ForegroundColor Red
    exit 1
}
