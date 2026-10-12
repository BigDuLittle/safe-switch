# Safe Switch 快速启动（不编译）
$exe = "C:\Users\admin\Desktop\tuomin\safe-switch\src-tauri\target\release\safe-switch.exe"
Get-Process safe-switch -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1
Start-Process $exe
Write-Host "已启动" -ForegroundColor Green
