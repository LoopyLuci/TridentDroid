# Emulator Dev Agent — Architecture Review Notes

This document reproduces the key technical review from the Emulator Dev Agent that shaped the final design. Treat these notes as binding constraints for the implementation.

---

## 1. fork_instance — Critical Ordering

The correct sequence for forking a KVM VM without memory corruption is:

1. **Pause ALL vCPUs first.** Not just one — every vCPU must be out of guest execution before the dirty log is touched. A single running vCPU can modify memory between your `sync_dirty_log` and `get_dirty_log` calls, producing a torn snapshot.

2. **Call `KVM_KVMCLOCK_CTRL` per vCPU.** This drains any pending timer interrupts and fully quiesces the virtual clock. Without this, a timer interrupt can fire in the child VM with a stale clocksource and cause the Android kernel's watchdog to trigger.

3. **`sync_dirty_log` THEN `get_dirty_log`.** The sync ensures that all CPU-side dirty bits are flushed into KVM's bitmap before you read it. Reversing these produces incomplete dirty page sets.

4. **Mark child slots `KVM_MEM_READONLY`.** This is how you get COW — not via `fork(2)` or `mmap(MAP_PRIVATE)`. KVM's `KVM_MEM_READONLY` causes any guest write to exit to the VMM (`KVM_EXIT_MMIO`) where you allocate a new page, copy from parent, and update the slot.

5. **Re-enable `KVM_CAP_MANUAL_DIRTY_LOG_PROTECT2` on the parent** after the child is set up. This prevents the parent's dirty log from accumulating unboundedly while the child is running.

### What goes wrong if you get this out of order

| Mistake | Symptom |
|---|---|
| Get dirty log before sync | Child misses dirty pages → stale data |
| kvmclock before pause | Race: vCPU resumes, clock jumps → guest panic |
| Forget READONLY flag | Child writes to parent memory → data corruption |
| Forget re-enable PROTECT2 | Parent's dirty bitmap grows indefinitely → OOM |

---

## 2. SR-IOV Direct Display

The RX 7900 XTX exposes VRAM as a mappable BAR0 region on each VF. The design:

```
Host:                                              Guest:
┌──────────────────────────────┐                ┌─────────────────────┐
│  Android SurfaceFlinger      │◄──virtio-vsock──│  SurfaceFlinger     │
│  (host-side render daemon)   │                │  (guest compositor) │
│          │                   │                └─────────────────────┘
│          ▼                   │
│  mmap(BAR0 resource0)        │
│          │                   │
│          ▼                   │
│  VF VRAM framebuffer         │──HDMI/DP──► Physical display
│  (24 GB VRAM @ PCIe BAR)     │
└──────────────────────────────┘
```

Key implementation points:
- Pass `amdgpu.dc=0` in the **guest** kernel command line. This tells amdgpu not to initialize the display controller inside the guest, leaving the host in full control of scanout.
- Open `/sys/bus/pci/devices/<vf>/resource0` with `O_RDWR` and `mmap` it `MAP_SHARED`. The `MAP_SHARED` flag is mandatory — `MAP_PRIVATE` would create an anonymous copy and bypass VRAM entirely.
- The VF must be bound to `vfio-pci` on the host before handing it to the guest. Binding to `amdgpu` on the host and then assigning to the guest will fail.
- Test with `write_test_pattern()` before wiring up SurfaceFlinger. A solid color on the physical display confirms the mapping is live.

---

## 3. mTLS Configuration

Common mistakes to avoid:
- **Do not** set `client_auth_optional(true)`. That degrades to one-way TLS and defeats the purpose.
- The server certificate's SAN must include `IP:127.0.0.1` and `IP:::1` for local CI. Without the SAN, Rust's `rustls` will reject the cert (CN-only matching is deprecated).
- Client certificates must be signed by the same CA the server trusts. Using a separate CA per client is fine but requires the server to trust all of them.
- Rotate certificates before their expiry. The `gen_certs.sh` script sets 10-year validity — acceptable for development but not production.

---

## 4. Performance Notes

- Use `RUSTFLAGS="-C target-cpu=native"` to enable AVX2/AVX-512 in the VMM hot paths (dirty page scanning, frame copying).
- The fork loop (12 instances) is I/O-bound on EPT TLB shootdowns. Use `KVM_CAP_DIRTY_LOG_RING` (available in Linux 5.11+) instead of the bitmap interface to reduce shootdown overhead.
- For the display path, avoid copying frames on the host. The DMA-BUF approach (BAR0 → udmabuf → VA-API encoder) is zero-copy end-to-end.
