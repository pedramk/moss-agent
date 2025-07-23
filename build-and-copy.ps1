#!/usr/bin/env pwsh
# Build script that compiles the Rust agent and copies it to the Electron app

Write-Host "Building Rust agent in release mode..." -ForegroundColor Green
cargo build --release

if ($LASTEXITCODE -eq 0) {
    Write-Host "Build successful! Copying agent.exe to app/bin/..." -ForegroundColor Green
    
    # Ensure the app/bin directory exists
    $appBinDir = "../app/bin"
    if (!(Test-Path $appBinDir)) {
        New-Item -ItemType Directory -Path $appBinDir -Force | Out-Null
        Write-Host "Created directory: $appBinDir" -ForegroundColor Yellow
    }
    
    # Copy the executable
    Copy-Item "target/release/agent.exe" "$appBinDir/agent.exe" -Force
    
    if (Test-Path "$appBinDir/agent.exe") {
        Write-Host "Successfully copied agent.exe to app/bin/" -ForegroundColor Green
        $fileInfo = Get-Item "$appBinDir/agent.exe"
        Write-Host "File size: $([math]::Round($fileInfo.Length / 1MB, 2)) MB" -ForegroundColor Cyan
        Write-Host "Last modified: $($fileInfo.LastWriteTime)" -ForegroundColor Cyan
    } else {
        Write-Host "Error: Failed to copy agent.exe" -ForegroundColor Red
        exit 1
    }
} else {
    Write-Host "Build failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}

Write-Host "Build and copy process completed successfully!" -ForegroundColor Green