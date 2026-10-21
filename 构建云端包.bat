@echo off
rem 双击即可触发云端全平台打包。
rem
rem 这个脚本**不打包**：它调用 scripts\build-cloud.ps1，由 GitHub Actions 出
rem Windows / Linux / macOS(双架构) / Android 四套产物。
rem
rem 自动打包只在推 v* tag 时发生；这里触发的是手动模式（workflow_dispatch），
rem 产物留在 Actions 页面，不会创建 Release。要附到 Release 请用命令行：
rem     powershell -NoProfile -ExecutionPolicy Bypass -File scripts\build-cloud.ps1 -ReleaseTag v3.10.0
rem
rem 要为大版本（x.0.0）打 tag 自动出包并建 Release：
rem     powershell -NoProfile -ExecutionPolicy Bypass -File scripts\build-cloud.ps1 -PushTag

setlocal
cd /d "%~dp0"

where powershell >nul 2>nul
if errorlevel 1 (
  echo 错误: 找不到 powershell。需要 Windows PowerShell 5.1 或更高版本。
  pause
  exit /b 1
)

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\build-cloud.ps1" %*
set "exit_code=%errorlevel%"

rem 双击运行时窗口会立刻关闭，这里停一下让人看到结果。
if "%exit_code%" neq "0" (
  echo.
  echo 构建触发失败（退出码 %exit_code%）。
) else (
  echo.
  echo 完成。产物在 GitHub Actions 页面查看。
)
pause
exit /b %exit_code%
