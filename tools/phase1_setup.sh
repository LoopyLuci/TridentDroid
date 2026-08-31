#!/usr/bin/env bash
# Phase 1.1 full setup script for TridentDroid on Ubuntu 24.04 LTS.
#
# Run as a normal user (sudo invoked where needed).
# Usage: bash tools/phase1_setup.sh [--skip-kernel] [--skip-iommu]
#
# Flags:
#   --skip-kernel   Skip kernel build (use existing guest_kernel file)
#   --skip-iommu    Skip IOMMU/SR-IOV setup (useful on VMs or without RX 7900 XTX)
set -euo pipefail

SKIP_KERNEL=false
SKIP_IOMMU=false
for arg in "$@"; do
    case "$arg" in
        --skip-kernel) SKIP_KERNEL=true ;;
        --skip-iommu)  SKIP_IOMMU=true ;;
    esac
done

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
log() { printf '\n\033[1;36m[TridentDroid Phase 1.1] %s\033[0m\n' "$*"; }
ok()  { printf '\033[1;32m  ✓ %s\033[0m\n' "$*"; }
err() { printf '\033[1;31m  ✗ %s\033[0m\n' "$*" >&2; exit 1; }

# ── 0. Prerequisites ──────────────────────────────────────────────────────────

log "Checking prerequisites..."

# KVM
if [[ ! -e /dev/kvm ]]; then
    err "/dev/kvm not found. Enable KVM in BIOS and ensure 'kvm_amd' module is loaded."
fi
ok "/dev/kvm present"

# KVM group membership
if ! groups | grep -q kvm; then
    log "Adding $USER to kvm group..."
    sudo usermod -aG kvm "$USER"
    ok "Added to kvm group (re-login or run: newgrp kvm)"
fi

# Rust
if ! command -v cargo &>/dev/null; then
    log "Installing Rust stable..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
fi
RUST_VERSION=$(rustc --version)
ok "Rust: $RUST_VERSION"

# Build tools for kernel
if ! command -v make &>/dev/null; then
    log "Installing build dependencies..."
    sudo apt-get install -y build-essential libncurses-dev bison flex libssl-dev \
        libelf-dev bc dwarves pahole git
fi

# ── 1. IOMMU & SR-IOV ────────────────────────────────────────────────────────

if [[ "$SKIP_IOMMU" == "false" ]]; then
    log "Checking IOMMU..."

    if ! dmesg | grep -qi "iommu"; then
        log "IOMMU not active in dmesg — adding kernel parameters..."
        GRUB_FILE=/etc/default/grub
        if grep -q 'amd_iommu=on' "$GRUB_FILE"; then
            ok "IOMMU params already in GRUB"
        else
            sudo sed -i \
                's/GRUB_CMDLINE_LINUX="\(.*\)"/GRUB_CMDLINE_LINUX="\1 amd_iommu=on iommu=pt"/' \
                "$GRUB_FILE"
            sudo update-grub
            echo ""
            echo "  GRUB updated. Please reboot and re-run this script."
            echo "  Command: sudo reboot"
            exit 0
        fi
    fi
    ok "IOMMU active"

    log "Scanning IOMMU groups for RX 7900 XTX..."
    bash "$REPO_ROOT/tools/iommu_viewer.sh" | grep -i "7900" || \
        echo "  (RX 7900 XTX not found — SR-IOV will be skipped)"

    # Enable SR-IOV VFs if the GPU is present and sriov_numvfs exists
    GPU_SRIOV_PATH=$(find /sys/class/drm -name 'sriov_numvfs' 2>/dev/null | head -1 || true)
    if [[ -n "$GPU_SRIOV_PATH" ]]; then
        CURRENT_VFS=$(cat "$GPU_SRIOV_PATH")
        if [[ "$CURRENT_VFS" -eq 0 ]]; then
            log "Enabling 16 SR-IOV virtual functions..."
            echo 16 | sudo tee "$GPU_SRIOV_PATH" > /dev/null
        fi
        ok "SR-IOV VFs: $(cat "$GPU_SRIOV_PATH")"
        lspci | grep -i "7900" || true
    else
        echo "  sriov_numvfs not found — skip SR-IOV (no RX 7900 XTX or driver not loaded)"
    fi
fi

# ── 2. Build minimal guest kernel ────────────────────────────────────────────

if [[ "$SKIP_KERNEL" == "false" ]]; then
    GUEST_KERNEL="$REPO_ROOT/guest_kernel"

    if [[ -f "$GUEST_KERNEL" ]]; then
        ok "guest_kernel already exists — skipping build (use --skip-kernel to force)"
    else
        log "Cloning Linux 6.6 (shallow)..."
        LINUX_DIR="/tmp/linux-trident"
        if [[ ! -d "$LINUX_DIR" ]]; then
            git clone --depth=1 --branch v6.6 \
                https://github.com/torvalds/linux.git "$LINUX_DIR"
        fi

        log "Configuring kernel..."
        cd "$LINUX_DIR"
        make defconfig kvm_guest.config

        # Minimal config for Phase 1.1 (no modules, serial console only)
        scripts/config --disable MODULES
        scripts/config --disable USB
        scripts/config --disable WLAN
        scripts/config --disable SOUND
        scripts/config --disable DRM
        scripts/config --disable NET
        scripts/config --disable BLOCK
        scripts/config --enable  SERIAL_8250
        scripts/config --enable  SERIAL_8250_CONSOLE
        scripts/config --enable  EARLY_PRINTK
        # Accept defaults for new symbols introduced by above changes
        make olddefconfig

        log "Building kernel (this takes 1-3 minutes on the 5900X)..."
        make -j"$(nproc)"

        cp arch/x86/boot/bzImage "$GUEST_KERNEL"
        cd "$REPO_ROOT"
        ok "Guest kernel built: $(du -sh "$GUEST_KERNEL" | cut -f1)"
    fi
fi

# ── 3. Generate mTLS certificates ────────────────────────────────────────────

if [[ ! -f "$REPO_ROOT/certs/server.crt" ]]; then
    log "Generating mTLS certificates..."
    bash "$REPO_ROOT/tools/gen_certs.sh"
fi
ok "mTLS certificates present"

# ── 4. Build TridentDroid ────────────────────────────────────────────────────

log "Building TridentDroid (release, native CPU)..."
cd "$REPO_ROOT"
RUSTFLAGS="-C target-cpu=native" cargo build --release 2>&1
ok "Build successful: $(du -sh target/release/tridentd | cut -f1)"

# ── 5. Phase 1.1 milestone — boot guest kernel ───────────────────────────────

log "=== PHASE 1.1 MILESTONE: booting guest kernel ==="
echo ""
echo "  Expected output: Linux boot messages ending with:"
echo "  'Kernel panic - not syncing: VFS: Unable to mount root fs'"
echo "  (This is SUCCESS — the kernel ran under KVM and printed to our VMM.)"
echo ""

GUEST_KERNEL="$REPO_ROOT/guest_kernel"
if [[ ! -f "$GUEST_KERNEL" ]]; then
    err "guest_kernel not found. Run without --skip-kernel or copy bzImage manually."
fi

exec "$REPO_ROOT/target/release/tridentd" \
    --vm-single \
    --kernel "$GUEST_KERNEL" \
    --vcpus 4 \
    --mem 512 \
    --args "console=ttyS0 earlyprintk=serial,ttyS0 panic=-1"
