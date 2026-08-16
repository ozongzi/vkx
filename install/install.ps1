# vkx 环境安装器（Windows PowerShell）
#
#   Set-ExecutionPolicy Bypass -Scope Process -Force; iwr -useb https://yinli.tech/file/install.ps1 | iex
#
# 从镜像装齐一整套开发环境到 %USERPROFILE%\.vkx，不需要管理员权限：
#   vkx 本体、CMake、Ninja、slangc、JDK、Gradle、Android SDK/NDK、
#   llvm-mingw（免装 Visual Studio 的 C++ 工具链）、以及 SDL3 等依赖的源码缓存。
#
# 环境变量：
#   VKX_MIRROR   镜像地址
#   VKX_HOME     安装目录，默认 %USERPROFILE%\.vkx
#   VKX_FORCE=1  已装的组件也重新安装
#
# 参数：
#   -NoAndroid   跳过 Android 相关组件（省约 5 GB）
#   -NoVkx       只装环境，不装 vkx 本体（自己开发 vkx 时用）

param(
    [switch]$NoAndroid,
    [switch]$NoVkx,
    [switch]$Force
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$DefaultMirror = "https://yinli.tech/file"
$mirror = if ($env:VKX_MIRROR) { $env:VKX_MIRROR } else { $DefaultMirror }
$mirror = $mirror.TrimEnd("/")
$vkxHome = if ($env:VKX_HOME) { $env:VKX_HOME } else { Join-Path $env:USERPROFILE ".vkx" }
if ($env:VKX_FORCE -eq "1") { $Force = $true }

function Write-Step($text) { Write-Host "==> " -ForegroundColor Green -NoNewline; Write-Host $text }
function Write-Info($text) { Write-Host "    $text" }
function Write-Warn($text) { Write-Host "警告: " -ForegroundColor Yellow -NoNewline; Write-Host $text }
function Fail($text) { Write-Host "错误: " -ForegroundColor Red -NoNewline; Write-Host $text; exit 1 }

# tar.exe 自 Windows 10 1803 起内置，解压全靠它。
if (-not (Get-Command tar -ErrorAction SilentlyContinue)) {
    Fail "找不到 tar 命令，需要 Windows 10 1803 或更高版本"
}

# --- 识别平台 --------------------------------------------------------------

$arch = switch ($env:PROCESSOR_ARCHITECTURE) {
    "AMD64" { "x86_64" }
    "ARM64" { "aarch64" }
    default { Fail "不支持的架构: $env:PROCESSOR_ARCHITECTURE" }
}
$platform = "windows-$arch"

Write-Step "vkx 环境安装"
Write-Info "平台   $platform"
Write-Info "镜像   $mirror"
Write-Info "目录   $vkxHome"

if ($mirror -eq $DefaultMirror -and -not $env:VKX_MIRROR) {
    Write-Warn "还在用占位镜像地址，请用 `$env:VKX_MIRROR 指定你自己的镜像"
}

# --- 取清单 ----------------------------------------------------------------

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("vkx-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $work -Force | Out-Null
New-Item -ItemType Directory -Path $vkxHome -Force | Out-Null

$manifestPath = Join-Path $work "manifest.txt"
try {
    Invoke-WebRequest -Uri "$mirror/manifest.txt" -OutFile $manifestPath -UseBasicParsing
} catch {
    Fail @"
取不到清单 $mirror/manifest.txt
       确认镜像地址可访问，或设置 `$env:VKX_MIRROR=<地址> 后重新运行。
"@
}

$installedPath = Join-Path $vkxHome "installed.txt"
$installed = @{}
if (Test-Path $installedPath) {
    foreach ($line in Get-Content $installedPath) {
        $f = $line -split "`t"
        if ($f.Count -ge 2) { $installed[$f[0]] = $f[1] }
    }
}

function Save-Installed {
    $lines = $installed.GetEnumerator() | Sort-Object Name | ForEach-Object { "$($_.Key)`t$($_.Value)" }
    Set-Content -Path $installedPath -Value $lines -Encoding ascii
}

function Install-Component($name, $version, $path, $wantSha, $dest) {
    if (-not $Force -and $installed[$name] -eq $version) {
        Write-Info ("{0,-24} {1}  已是最新" -f $name, $version)
        return
    }

    Write-Step "$name $version"
    $archive = Join-Path $work ([System.IO.Path]::GetFileName($path))
    try {
        Invoke-WebRequest -Uri "$mirror/$path" -OutFile $archive -UseBasicParsing
    } catch {
        Fail "下载失败: $mirror/$path"
    }

    $gotSha = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLower()
    if ($gotSha -ne $wantSha.ToLower()) {
        Fail @"
$name 校验失败
       期望 $wantSha
       实际 $gotSha
       镜像上的文件可能损坏或被改动过。
"@
    }

    $target = Join-Path $vkxHome $dest
    if (Test-Path $target) { Remove-Item $target -Recurse -Force }
    New-Item -ItemType Directory -Path $target -Force | Out-Null
    & tar -xzf $archive -C $target
    if ($LASTEXITCODE -ne 0) { Fail "$name 解压失败" }
    Remove-Item $archive -Force

    $installed[$name] = $version
    Save-Installed
}

# --- 逐个安装 --------------------------------------------------------------

$ndkVersion = ""
$count = 0
foreach ($line in Get-Content $manifestPath) {
    if (-not $line -or $line.StartsWith("#")) { continue }
    $f = $line -split "`t"
    if ($f.Count -lt 6) { continue }
    $name, $entryPlatform, $version, $path, $sha, $dest = $f

    if ($entryPlatform -ne $platform -and $entryPlatform -ne "any") { continue }
    if ($NoAndroid -and ($name -like "android-*" -or $name -eq "jdk" -or $name -eq "gradle" -or $name -eq "sdl-android")) { continue }
    if ($NoVkx -and $name -eq "vkx") { continue }

    Install-Component $name $version $path $sha $dest
    if ($name -eq "android-ndk") { $ndkVersion = $version }
    $count++
}

if ($count -eq 0) { Fail "清单里没有适用于 $platform 的组件" }

# Gradle 在需要补装组件时会检查许可文件，SDK 是我们直接铺好的，先把许可补上。
if (-not $NoAndroid) {
    $licenses = Join-Path $vkxHome "android\sdk\licenses"
    New-Item -ItemType Directory -Path $licenses -Force | Out-Null
    Set-Content -Path (Join-Path $licenses "android-sdk-license") -Encoding ascii -Value @(
        "", "8933bad161af4178b1185d1a37fbf41ea5269c55",
        "d56f5187479451eabf01fb78af6dfcb131a6481e",
        "24333f8a63b6825ea9c5514f83c2829b004d1fee")
    Set-Content -Path (Join-Path $licenses "android-sdk-preview-license") -Encoding ascii -Value @(
        "", "84831b9409646a918e30573bab4c9c91346d8abd")
}

Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue

# --- 写用户级环境变量 --------------------------------------------------------

function Set-UserEnv($name, $value) {
    [Environment]::SetEnvironmentVariable($name, $value, "User")
    Set-Item -Path "env:$name" -Value $value
}

Set-UserEnv "VKX_HOME" $vkxHome
if (-not $NoAndroid) {
    Set-UserEnv "JAVA_HOME" (Join-Path $vkxHome "tools\jdk")
    Set-UserEnv "ANDROID_HOME" (Join-Path $vkxHome "android\sdk")
    if ($ndkVersion) {
        Set-UserEnv "ANDROID_NDK_HOME" (Join-Path $vkxHome "android\sdk\ndk\$ndkVersion")
    }
}

$pathEntries = @(
    (Join-Path $vkxHome "bin"),
    (Join-Path $vkxHome "tools\cmake\bin"),
    (Join-Path $vkxHome "tools\ninja"),
    (Join-Path $vkxHome "tools\slang\bin")
)
if (-not $NoAndroid) {
    $pathEntries += (Join-Path $vkxHome "tools\gradle\bin")
    $pathEntries += (Join-Path $vkxHome "android\sdk\platform-tools")
    $pathEntries += (Join-Path $vkxHome "tools\jdk\bin")
}

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (-not $userPath) { $userPath = "" }
$added = @()
foreach ($entry in $pathEntries) {
    if ($userPath -notlike "*$entry*") { $added += $entry }
}
if ($added.Count -gt 0) {
    $userPath = ($added -join ";") + ";" + $userPath
    [Environment]::SetEnvironmentVariable("Path", $userPath, "User")
}
$env:Path = ($pathEntries -join ";") + ";" + $env:Path

# --- 装不进 ~/.vkx 的东西 ----------------------------------------------------

$missing = $false

# C++ 工具链：llvm-mingw 已经装好了，所以这里只是提示更常用的 MSVC。
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path $vswhere) {
    Write-Info "检测到 Visual Studio，vkx 会优先用 MSVC 构建"
} else {
    Write-Info "未检测到 Visual Studio，vkx 会用刚装好的 llvm-mingw 构建"
}

# Vulkan 驱动：vulkan-1.dll 由显卡驱动提供，装不进用户目录。
if (-not (Test-Path "$env:SystemRoot\System32\vulkan-1.dll")) {
    $missing = $true
    Write-Warn "没有找到 vulkan-1.dll，说明显卡驱动没带 Vulkan 支持"
    Write-Info "请到显卡厂商官网更新驱动（NVIDIA / AMD / Intel）"
}

# --- 自检 ------------------------------------------------------------------
# 装完立刻验一遍，别等到用户第一次构建时才发现哪个包是坏的。

Write-Step "自检"
function Test-Tool($label, $exe, $arguments) {
    if (-not (Test-Path $exe)) {
        $script:missing = $true
        Write-Warn "$label 没装上：$exe"
        return
    }
    & $exe @arguments *> $null
    if ($LASTEXITCODE -eq 0) {
        Write-Info ("{0,-8} 可用" -f $label)
    } else {
        $script:missing = $true
        Write-Warn "$label 装上了却跑不起来：$exe"
    }
}
if (-not $NoVkx) { Test-Tool "vkx" (Join-Path $vkxHome "bin\vkx.exe") @("--version") }
Test-Tool "cmake"  (Join-Path $vkxHome "tools\cmake\bin\cmake.exe") @("--version")
Test-Tool "ninja"  (Join-Path $vkxHome "tools\ninja\ninja.exe") @("--version")
Test-Tool "slangc" (Join-Path $vkxHome "tools\slang\bin\slangc.exe") @("-h")
if (-not $NoAndroid) {
    Test-Tool "java" (Join-Path $vkxHome "tools\jdk\bin\java.exe") @("-version")
}

# --- 收尾 ------------------------------------------------------------------

Write-Host ""
Write-Host "安装完成。" -ForegroundColor White
Write-Host ""
Write-Info "环境变量已写入用户配置，新开的终端才生效。"
Write-Host ""
Write-Host "  vkx new mygame   # 新建工程"
Write-Host "  cd mygame; vkx run"
Write-Host ""
if ($missing) {
    Write-Warn "上面还有需要你手动处理的东西，处理完重新运行本脚本即可。"
}
Write-Host "全部文件都在 $vkxHome，删掉这个目录就等于卸载干净。" -ForegroundColor DarkGray
