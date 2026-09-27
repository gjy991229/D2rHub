@echo off
chcp 65001 >nul
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0release.ps1" %*
set "release_exit=%errorlevel%"
echo.
pause
exit /b %release_exit%
