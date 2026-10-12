# Safe Switch 一键构建脚本
# 用法: .\build.ps1 [--run]
#   --run  编译完成后自动启动

param(
  [switch]$Run
)

$ErrorActionPreference = "Stop"
$ProjectRoot = $PSScriptRoot

Write-Host "=== Safe Switch 构建脚本 ===" -ForegroundColor Cyan

# 1. 设置编译环境
Write-Host "[1/4] 设置编译环境..." -ForegroundColor Yellow
$sdk = "C:\Program Files (x86)\Windows Kits\10"
$env:INCLUDE = "$sdk\Include\10.0.14393.0\ucrt;$sdk\Include\10.0.14393.0\um;$sdk\Include\10.0.14393.0\shared"
$env:LIB = "$sdk\Lib\10.0.14393.0\ucrt\x64;$sdk\Lib\10.0.14393.0\um\x64"
$env:CC = "C:\Users\admin\Desktop\tuomin\llvm\bin\clang-cl.exe"
$env:CXX = "C:\Users\admin\Desktop\tuomin\llvm\bin\clang-cl.exe"
$env:CXXFLAGS = "-EHsc"
$env:VCINSTALLDIR = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\"
$env:WindowsSdkDir = "$sdk\"
$env:WindowsSDKVersion = "10.0.14393.0\"

# 2. 构建前端
Write-Host "[2/4] 构建前端..." -ForegroundColor Yellow
Set-Location $ProjectRoot
pnpm run build:renderer
if ($LASTEXITCODE -ne 0) { Write-Host "前端构建失败" -ForegroundColor Red; exit 1 }

# 3. 构建 Rust 后端
Write-Host "[3/4] 编译 Rust 后端 (release)..." -ForegroundColor Yellow
Set-Location "$ProjectRoot\src-tauri"
cargo build --release
if ($LASTEXITCODE -ne 0) { Write-Host "Rust 编译失败" -ForegroundColor Red; exit 1 }

# 4. 完成
$exe = "$ProjectRoot\src-tauri\target\release\safe-switch.exe"
$sizeMB = [math]::Round((Get-Item $exe).Length / 1MB, 1)
Write-Host "[4/4] 构建完成! exe: $exe ($sizeMB MB)" -ForegroundColor Green

if ($Run) {
  Write-Host "启动程序..." -ForegroundColor Cyan
  Get-Process safe-switch -ErrorAction SilentlyContinue | Stop-Process -Force
  Start-Sleep -Seconds 1
  Start-Process $exe
}
