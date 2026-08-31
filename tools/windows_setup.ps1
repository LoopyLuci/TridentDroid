# TridentDroid — Windows 10 development environment setup
# Run once from an elevated PowerShell prompt:
#   Set-ExecutionPolicy Bypass -Scope Process -Force
#   .\tools\windows_setup.ps1
#
# What this does:
#   1. Enables Windows Hypervisor Platform (WHP) optional feature
#   2. Installs Rust (MSVC toolchain) via rustup
#   3. Verifies WHP DLLs and Hyper-V service
#   4. Builds the workspace
#   5. Checks for a guest_kernel and runs the Phase 1.1 milestone

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Log  { Write-Host "[TridentDroid] $args" -ForegroundColor Cyan }
function Ok   { Write-Host "  OK  $args" -ForegroundColor Green }
function Warn { Write-Host "  WARN $args" -ForegroundColor Yellow }
function Fail { Write-Host "  FAIL $args" -ForegroundColor Red; exit 1 }

$RepoRoot = Split-Path $PSScriptRoot -Parent

# ── 1. Windows Hypervisor Platform ───────────────────────────────────────────

Log "Checking Windows Hypervisor Platform..."

$whpDll = "C:\Windows\System32\WinHvPlatform.dll"
$whpEmu = "C:\Windows\System32\WinHvEmulation.dll"

if ((Test-Path $whpDll) -and (Test-Path $whpEmu)) {
    Ok "WHP DLLs present ($whpDll)"
} else {
    Log "Enabling Windows Hypervisor Platform optional feature (requires reboot)..."
    $result = Enable-WindowsOptionalFeature -Online -FeatureName HypervisorPlatform -NoRestart
    if ($result.RestartNeeded) {
        Warn "A reboot is required to complete WHP installation. Reboot and re-run this script."
        exit 0
    }
}

# Verify Hyper-V service is running (required for WHP)
$svc = Get-Service vmms -ErrorAction SilentlyContinue
if ($svc -and $svc.Status -eq "Running") {
    Ok "Hyper-V VM Management service is running"
} else {
    Warn "Hyper-V service (vmms) not running. WHP requires Hyper-V to be enabled."
    Warn "Enable Hyper-V in Windows Features and reboot, then re-run this script."
}

# ── 2. Rust (MSVC toolchain) ──────────────────────────────────────────────────

Log "Checking Rust installation..."

if (Get-Command cargo -ErrorAction SilentlyContinue) {
    $rv = cargo --version
    Ok "Rust already installed: $rv"
} else {
    Log "Installing Rust (stable-x86_64-pc-windows-msvc)..."

    $rustupInit = "$env:TEMP\rustup-init.exe"
    Invoke-WebRequest -Uri "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe" `
                      -OutFile $rustupInit -UseBasicParsing

    # Install quietly with MSVC toolchain (no MinGW needed — uses MSVC linker)
    & $rustupInit -y --default-toolchain stable --default-host x86_64-pc-windows-msvc `
                     --profile minimal 2>&1

    # Reload PATH for this session
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

    if (Get-Command cargo -ErrorAction SilentlyContinue) {
        Ok "Rust installed: $(cargo --version)"
    } else {
        Fail "Rust installation failed. Check $rustupInit output above."
    }
}

# Ensure clippy and rustfmt are available
rustup component add clippy rustfmt 2>&1 | Out-Null
Ok "rustfmt and clippy present"

# ── 3. Visual Studio Build Tools (MSVC linker) ────────────────────────────────

Log "Checking MSVC linker (link.exe)..."
$vsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$linkExe = $null

if (Test-Path $vsWhere) {
    $vsPath = & $vsWhere -latest -products * -requires Microsoft.VisualCpp.Tools.HostX64.TargetX64 `
                         -property installationPath 2>$null
    if ($vsPath) {
        $linkExe = Get-ChildItem "$vsPath" -Filter "link.exe" -Recurse -ErrorAction SilentlyContinue |
                   Select-Object -First 1 -ExpandProperty FullName
    }
}

if ($linkExe) {
    Ok "MSVC link.exe: $linkExe"
} else {
    Warn "MSVC Build Tools not found."
    Warn "Install from: https://aka.ms/vs/17/release/vs_buildtools.exe"
    Warn "Select: C++ build tools workload + Windows 11 SDK"
    Warn "After installing, re-run this script."
    # Don't exit — cargo will give a clear error at build time
}

# ── 4. Build ──────────────────────────────────────────────────────────────────

Log "Building TridentDroid workspace (release)..."
Set-Location $RepoRoot
$env:RUSTFLAGS = "-C target-cpu=native"

cargo build --workspace --release 2>&1
if ($LASTEXITCODE -ne 0) { Fail "cargo build failed — see output above" }
Ok "Build succeeded: $(Get-Item '.\target\release\tridentd.exe' | Select-Object -ExpandProperty Length) bytes"

# ── 5. Phase 1.1 milestone ────────────────────────────────────────────────────

Log "=== PHASE 1.1 MILESTONE: booting guest kernel on Windows/WHP ==="
Write-Host ""
Write-Host "  Expected: WHvCreatePartition OK -> Linux boot log -> Kernel panic (VFS)"
Write-Host "  That panic = SUCCESS. The kernel ran under WHP."
Write-Host ""

$guestKernel = "$RepoRoot\guest_kernel"
if (-not (Test-Path $guestKernel)) {
    Warn "guest_kernel not found at $guestKernel"
    Warn "Build it on Linux with: make phase1 --skip-iommu"
    Warn "Then copy bzImage here as 'guest_kernel'"
    Warn ""
    Warn "Quick alternative: use WSL2 to build it:"
    Warn "  wsl bash tools/phase1_setup.sh --skip-iommu"
    exit 0
}

& ".\target\release\tridentd.exe" `
    --vm-single `
    --kernel "$guestKernel" `
    --vcpus 1 `
    --mem 512 `
    --args "console=ttyS0 earlyprintk=serial panic=-1"
