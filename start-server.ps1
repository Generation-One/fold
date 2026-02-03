# Fold Server Starter
# Builds and starts the server, waiting for the success message

param(
    [switch]$NoBuild,
    [int]$Timeout = 300  # seconds to wait for startup
)

$ErrorActionPreference = "Stop"
$serverDir = $PSScriptRoot
$logFile = "$env:TEMP\fold-server.log"

Push-Location $serverDir

try {
    # Build unless skipped
    if (-not $NoBuild) {
        Write-Host "Building Fold server..." -ForegroundColor Cyan
        # Temporarily allow errors so cargo warnings don't terminate
        $ErrorActionPreference = "Continue"
        $buildResult = cargo build 2>&1
        $buildExitCode = $LASTEXITCODE
        $ErrorActionPreference = "Stop"
        if ($buildExitCode -ne 0) {
            Write-Host "Build failed!" -ForegroundColor Red
            $buildResult | Select-Object -Last 30
            exit 1
        }
        Write-Host "Build complete." -ForegroundColor Green
    }

    # Clear old logs
    $logFileErr = "$env:TEMP\fold-server-err.log"
    if (Test-Path $logFile) { Remove-Item $logFile }
    if (Test-Path $logFileErr) { Remove-Item $logFileErr }

    # Start server in background (WorkingDirectory ensures .env is loaded)
    Write-Host "Starting Fold server..." -ForegroundColor Cyan
    $process = Start-Process -FilePath "cargo" -ArgumentList "run" -WorkingDirectory $serverDir -RedirectStandardOutput $logFile -RedirectStandardError $logFileErr -PassThru -WindowStyle Hidden

    # Wait for success message
    $startTime = Get-Date
    $success = $false

    while (((Get-Date) - $startTime).TotalSeconds -lt $Timeout) {
        Start-Sleep -Milliseconds 500

        if (Test-Path $logFile) {
            $content = Get-Content $logFile -Raw -ErrorAction SilentlyContinue
            if ($content -match "FOLD SERVER STARTED SUCCESSFULLY") {
                $success = $true
                break
            }
        }
        # Check stderr for errors
        if (Test-Path $logFileErr) {
            $errContent = Get-Content $logFileErr -Raw -ErrorAction SilentlyContinue
            if ($errContent -match "error\[E") {
                Write-Host "Server failed to start!" -ForegroundColor Red
                Get-Content $logFileErr | Select-Object -Last 20
                Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
                exit 1
            }
        }

        # Check if process died
        if ($process.HasExited) {
            Write-Host "Server process exited unexpectedly!" -ForegroundColor Red
            if (Test-Path $logFile) { Get-Content $logFile | Select-Object -Last 20 }
            if (Test-Path $logFileErr) { Get-Content $logFileErr | Select-Object -Last 20 }
            exit 1
        }
    }

    if ($success) {
        Write-Host "========================================" -ForegroundColor Green
        Write-Host "  FOLD SERVER STARTED SUCCESSFULLY" -ForegroundColor Green
        Write-Host "  PID: $($process.Id)" -ForegroundColor Green
        Write-Host "  Log: $logFile" -ForegroundColor Green
        Write-Host "========================================" -ForegroundColor Green
        Write-Host ""
        Write-Host "Server running at http://localhost:8765" -ForegroundColor Cyan
        Write-Host "To stop: Stop-Process -Id $($process.Id)" -ForegroundColor Yellow
        exit 0
    } else {
        Write-Host "Timeout waiting for server to start!" -ForegroundColor Red
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        exit 1
    }
}
finally {
    Pop-Location
}
