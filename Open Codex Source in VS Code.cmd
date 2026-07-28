@echo off
set "PATH=%USERPROFILE%\.cargo\bin;%USERPROFILE%\scoop\apps\mingw\current\bin;%PATH%"
set "RUSTUP_TOOLCHAIN=1.95.0-x86_64-pc-windows-gnu"
where code >nul 2>&1
if errorlevel 1 (
  echo VS Code command "code" was not found in PATH.
  exit /b 1
)
code --new-window "%~dp0Codex-Source.code-workspace"
