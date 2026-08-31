#!/bin/bash
# Build guest kernel in WSL2 for TridentDroid Phase 1.1
# Copy bzImage to /mnt/z/Projects/TridentDroidEmulator/guest_kernel
set -e

LOGFILE="/tmp/trident_kernel_build.log"
exec > >(tee -a "$LOGFILE") 2>&1

echo "[$(date)] === Step 1: Remove stale dpkg locks ==="
sudo rm -f /var/lib/dpkg/lock-frontend /var/lib/dpkg/lock \
           /var/cache/apt/archives/lock /var/lib/apt/lists/lock 2>/dev/null || true

echo "[$(date)] === Step 2: Install build dependencies ==="
DEBIAN_FRONTEND=noninteractive sudo apt-get update -qq
DEBIAN_FRONTEND=noninteractive sudo apt-get install -y -qq \
    build-essential libncurses-dev bison flex libssl-dev libelf-dev \
    bc dwarves pahole git curl

echo "[$(date)] === Step 3: Install Rust ==="
if ! command -v cargo &>/dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --quiet --default-toolchain stable --profile minimal
fi
source "$HOME/.cargo/env" 2>/dev/null || true
cargo --version

echo "[$(date)] === Step 4: Clone Linux 6.6 (shallow) ==="
KDIR="/tmp/linux-minimal"
if [ ! -d "$KDIR" ]; then
    git clone --depth=1 --branch v6.6 \
        https://github.com/torvalds/linux.git "$KDIR"
else
    echo "  (already cloned, skipping)"
fi

echo "[$(date)] === Step 5: Build minimal guest kernel ==="
cd "$KDIR"
make defconfig kvm_guest.config
# Ensure COM1 serial + early printk
scripts/config --disable CONFIG_MODULES \
               --enable  CONFIG_SERIAL_8250 \
               --enable  CONFIG_SERIAL_8250_CONSOLE \
               --enable  CONFIG_EARLY_PRINTK
make olddefconfig
make -j"$(nproc)" 2>&1 | tail -20

echo "[$(date)] === Step 6: Copy bzImage ==="
DEST="/mnt/z/Projects/TridentDroidEmulator/guest_kernel"
cp arch/x86/boot/bzImage "$DEST"
echo "  Copied to $DEST ($(stat -c%s "$DEST") bytes)"

echo "[$(date)] === DONE — guest_kernel ready ==="
