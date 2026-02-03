# Stop Fold server
$processes = Get-Process | Where-Object { $_.ProcessName -eq "fold" -or ($_.ProcessName -eq "cargo" -and $_.CommandLine -match "run") }

if ($processes) {
    $processes | ForEach-Object {
        Write-Host "Stopping process $($_.Id)..." -ForegroundColor Yellow
        Stop-Process -Id $_.Id -Force
    }
    Write-Host "Fold server stopped." -ForegroundColor Green
} else {
    Write-Host "No Fold server process found." -ForegroundColor Yellow
}
