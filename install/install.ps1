# vkx 安装脚本（Windows）。
#
#   Set-ExecutionPolicy Bypass -Scope Process -Force; iwr -useb https://yinli.tech/file/install.ps1 | iex
#
# 只做一件事：把 vkx 放进 ~\.vkx\bin，并加进 PATH。
# 工具链由 vkx 自己按需下载，见 `vkx help fetch`。
$ErrorActionPreference = "Stop"

$mirror = if ($env:VKX_MIRROR) { $env:VKX_MIRROR } else { "https://yinli.tech/file" }
$vkxHome = if ($env:VKX_HOME) { $env:VKX_HOME } else { "$HOME\.vkx" }

$arch = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "arm64" } else { "x64" }
$platform = "windows-$arch"

Write-Host "==> 取 vkx ($platform)" -ForegroundColor Green
New-Item -ItemType Directory -Force -Path "$vkxHome\bin" | Out-Null
$version = (Invoke-WebRequest -UseBasicParsing "$mirror/vkx/version.txt").Content.Trim()
Invoke-WebRequest -UseBasicParsing "$mirror/vkx/$version/vkx-$version-$platform.exe" `
    -OutFile "$vkxHome\bin\vkx.exe"

Write-Host "==> 接进 PATH" -ForegroundColor Green
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$vkxHome\bin*") {
    [Environment]::SetEnvironmentVariable("Path", "$vkxHome\bin;$userPath", "User")
}

Write-Host ""
Write-Host "装好了：$vkxHome\bin\vkx.exe"
Write-Host ""
Write-Host "打开一个新终端，然后："
Write-Host ""
Write-Host "    vkx new mygame"
Write-Host "    cd mygame"
Write-Host "    vkx run"
Write-Host ""
Write-Host "第一次 vkx run 会下载编译需要的工具链（几十 MB）。"
Write-Host "想一次备齐（含 Android 那几 GB）：vkx fetch --all"
Write-Host "看环境齐不齐：vkx doctor"
