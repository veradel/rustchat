@echo off
if "%1"=="" (
    echo Usage: run-client.bat <username>
    echo Example: run-client.bat Alice
    pause
    exit /b 1
)
cargo run -- client --address 127.0.0.1:8080 --username %1
pause

