param([switch]$Run)
$ErrorActionPreference = 'Stop'
$nativeRoot = Split-Path -Parent $PSScriptRoot
if ($env:OS -ne 'Windows_NT') { throw 'This packaging script targets Windows.' }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'Install Rust with the MSVC toolchain first.' }
$originalFxc = $env:GPUI_FXC_PATH
$targetBinary = Join-Path $nativeRoot 'target/release/vesktop-gpui.exe'
if (Get-Process -Name 'vesktop-gpui' -ErrorAction SilentlyContinue | Where-Object Path -EQ $targetBinary) {
    throw 'Close the native client running from target/release before rebuilding.'
}
try {
    if (-not $env:GPUI_FXC_PATH) {
        $sdkRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
        $compiler = Get-ChildItem -LiteralPath $sdkRoot -Directory -ErrorAction SilentlyContinue |
            Where-Object Name -Match '^10\.' | Sort-Object { [version]$_.Name } -Descending |
            ForEach-Object { Join-Path $_.FullName 'x64/fxc.exe' } |
            Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
        if (-not $compiler) { throw 'Install a Windows SDK with x64/fxc.exe, or set GPUI_FXC_PATH explicitly.' }
        $env:GPUI_FXC_PATH = $compiler
    }
    & cargo build --manifest-path (Join-Path $nativeRoot 'Cargo.toml') --locked --release -p vesktop-gpui
    if ($LASTEXITCODE -ne 0) { throw 'Native release build failed.' }
    $distribution = Join-Path $nativeRoot 'dist'
    New-Item -ItemType Directory -Force -Path $distribution | Out-Null
    Copy-Item -LiteralPath (Join-Path $nativeRoot 'target/release/vesktop-gpui.exe') -Destination $distribution -Force
    Copy-Item -LiteralPath (Join-Path (Split-Path -Parent $nativeRoot) 'LICENSE') -Destination $distribution -Force
    foreach ($file in @('README.md', 'NOTICE', 'GPUI-INPUT-LICENSE')) {
        Copy-Item -LiteralPath (Join-Path $nativeRoot $file) -Destination $distribution -Force
    }
    Write-Output "Portable build: $distribution"
    if ($Run) { & (Join-Path $distribution 'vesktop-gpui.exe') }
} finally {
    $env:GPUI_FXC_PATH = $originalFxc
}
