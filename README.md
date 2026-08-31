# TridentDroid

A next‑generation, production‑grade Android emulator that is **ultra‑lightweight**, **robust**, and **minimal**. It targets developers and CI systems who need to run and test **any Android version** and **custom Android OS** with near‑native performance.

## Hardware Target

- CPU: AMD Ryzen 9 5900X (12c/24t, Zen 3)
- RAM: 64 GB DDR4
- GPU: AMD Radeon RX 7900 XTX (24 GB VRAM, RDNA 3, Vulkan 1.4)
- Host OS: Ubuntu 24.04 LTS (recommended) – KVM + SR‑IOV

## Core Capabilities

- **Instant boot**: <2 s cold boot, <200 ms snapshot/restore
- **60 FPS Android UI** with full GPU acceleration
- **Zero‑copy display streaming** via direct DMA‑BUF from the virtual function
- **Lightweight**: <512 MB RAM per idle instance, near‑zero host CPU usage when paused
- **Full automation**: gRPC API with mTLS, CI/CD friendly (GitHub Actions, Jenkins)
- **Any Android version**: from 1.0 to latest preview, including custom ROMs (LineageOS, GrapheneOS, etc.)
- **SR‑IOV GPU sharing**: each virtual machine gets a hardware‑backed virtual function of the RX 7900 XTX

## Quick Start

See [PLAN.md](PLAN.md) for the detailed build instructions.
