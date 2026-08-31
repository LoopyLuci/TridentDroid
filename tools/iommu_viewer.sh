#!/usr/bin/env bash
# Display IOMMU groups and their PCI devices.
# Pipe through grep to find the RX 7900 XTX:
#   ./tools/iommu_viewer.sh | grep -i vga
set -euo pipefail

shopt -s nullglob
for IOMMU_GROUP in /sys/kernel/iommu_groups/*/; do
    GROUP_NUM=$(basename "$IOMMU_GROUP")
    for DEVICE in "$IOMMU_GROUP"devices/*/; do
        PCI_ADDR=$(basename "$DEVICE")
        DRIVER=$(basename "$(readlink -f "$DEVICE/driver" 2>/dev/null || echo "-")") 2>/dev/null || DRIVER="-"
        DESC=$(lspci -s "$PCI_ADDR" 2>/dev/null | sed 's/^[^ ]* //' || echo "(unknown)")
        printf "IOMMU Group %3s: %s %-12s %s\n" "$GROUP_NUM" "$PCI_ADDR" "[$DRIVER]" "$DESC"
    done
done
