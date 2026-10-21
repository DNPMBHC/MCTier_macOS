@echo off
rem Double-click entry point that triggers the cloud build.
rem
rem This script does NOT build anything. It calls scripts\build-cloud.ps1, which
rem asks GitHub Actions to produce Windows / Linux / macOS (both arches) / Android.
rem
rem Automatic packaging only happens on v* tag pushes. This is the manual mode
rem (workflow_dispatch): artifacts stay in the Actions run and no Release is made.
rem To attach them to a Release, pass -ReleaseTag from a terminal:
rem     powershell -NoProfile -ExecutionPolicy Bypass -File scripts\build-cloud.ps1 -ReleaseTag v3.10.0
rem
rem To tag a major version (x.0.0) and let CI build + create the Release:
rem     powershell -NoProfile -ExecutionPolicy Bypass -File scripts\build-cloud.ps1 -PushTag
rem
rem NOTE: this file is deliberately ASCII-only. cmd.exe parses batch files using
rem the console codepage, so UTF-8 Chinese text here would render as mojibake on a
rem non-936 console. The Chinese messages live in build-cloud.ps1, which carries a
rem UTF-8 BOM and is read correctly by Windows PowerShell.

setlocal
cd /d "%~dp0"

where powershell >nul 2>nul
if errorlevel 1 (
  echo Error: powershell not found. Windows PowerShell 5.1 or newer is required.
  pause
  exit /b 1
)

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\build-cloud.ps1" %*
set "exit_code=%errorlevel%"

rem Double-clicked windows close immediately, so hold the window to show the result.
if "%exit_code%" neq "0" (
  echo.
  echo Cloud build trigger failed ^(exit code %exit_code%^).
) else (
  echo.
  echo Done. Check the GitHub Actions page for the artifacts.
)
pause
exit /b %exit_code%
