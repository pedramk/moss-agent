@echo off
echo Building Rust agent in release mode...
cargo build --release

if %errorlevel% neq 0 (
    echo Build failed!
    exit /b %errorlevel%
)

echo Build successful! Copying agent.exe to app/bin/...

REM Ensure the app/bin directory exists
if not exist "../app/bin" (
    mkdir "../app/bin"
    echo Created directory: ../app/bin
)

REM Copy the executable
copy "target\release\agent.exe" "..\app\bin\agent.exe" /Y >nul

if exist "..\app\bin\agent.exe" (
    echo Successfully copied agent.exe to app/bin/
) else (
    echo Error: Failed to copy agent.exe
    exit /b 1
)

echo Build and copy process completed successfully!