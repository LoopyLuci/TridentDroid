# TODO List

## Critical Path

- [ ] Phase 0: Environment setup and SR‑IOV activation.
- [ ] Phase 1.1: Minimal KVM VMM boots a "hello world" kernel.
- [ ] Phase 1.2: Allocate memory with dirty log, setup vCPUs.
- [ ] Phase 1.3: Boot Android GSI with virtio‑block and serial console.
- [ ] Phase 2.1: Map VF BAR0 and display a test pattern.
- [ ] Phase 2.2: Export DMA‑BUF fd and stream via gRPC.
- [ ] Phase 3.1: Implement pause‑dirty‑fork sequence exactly as reviewed.
- [ ] Phase 3.2: COW memory forking with write fault handler.
- [ ] Phase 3.3: Parallel device reset.
- [ ] Phase 3.4: Fork 12 instances and validate boot time.
- [ ] Phase 4.1: Write `tridentd.proto` and generate code.
- [ ] Phase 4.2: Implement gRPC server with mTLS.
- [ ] Phase 4.3: CI integration script.
- [ ] Phase 5: Custom ROM boot and sensors.

## Nice‑to‑Haves

- [ ] Snapshot disk incremental via qcow2.
- [ ] GPU‑accelerated video encoding for streaming.
- [ ] Windows host support (Hyper‑V backend) — future.

## Bugs to Watch

- [ ] Ensure vCPUs are fully quiesced before dirty log snapshot.
- [ ] Verify MSI‑X interrupts are cleared after fork.
- [ ] Test SR‑IOV reset after many reboot cycles (target 15+).
