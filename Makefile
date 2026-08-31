RUSTFLAGS := -C target-cpu=native

# Workspace-aware cargo invocation
CARGO := RUSTFLAGS="$(RUSTFLAGS)" cargo

.PHONY: all build release check test phase1 boot serve ci certs clean tag-phase1 tag-phase2

# ── Build ─────────────────────────────────────────────────────────────────────

all: build

build:
	$(CARGO) build --workspace

release:
	$(CARGO) build --workspace --release

check:
	$(CARGO) clippy --workspace -- -D warnings
	$(CARGO) fmt --all --check

# ── Tests ─────────────────────────────────────────────────────────────────────

test:
	$(CARGO) test --workspace -- --nocapture

# SR-IOV BAR0 test (Linux only — set VF_PCI_ADDR first)
test-sriov:
	VF_PCI_ADDR=$(VF_PCI_ADDR) $(CARGO) test -p tridentd test_sriov_mmap -- --nocapture

# Dirty-log smoke test (Linux only — requires /dev/kvm)
test-kvm:
	$(CARGO) test -p tridentd test_dirty_log -- --nocapture

# ── Phase shortcuts ───────────────────────────────────────────────────────────

# Full Phase 1.1: kernel build + TridentDroid build + first VM boot (Linux)
phase1:
	bash tools/phase1_setup.sh

# Re-boot existing guest_kernel without rebuilding (Linux)
boot:
	bash tools/phase1_setup.sh --skip-kernel --skip-iommu

# ── WHP smoke test (Windows — verify Hypervisor Platform is available) ────────
test-whp:
	$(CARGO) run -p tridentd -- --vm-single --kernel guest_kernel --vcpus 1 --mem 512 \
	    --args "console=ttyS0 earlyprintk=serial panic=-1"

# ── gRPC daemon ───────────────────────────────────────────────────────────────

serve: release certs
	./target/release/tridentd --serve

# ── Certificates ──────────────────────────────────────────────────────────────

certs:
	bash tools/gen_certs.sh

# ── CI harness ────────────────────────────────────────────────────────────────

ci:
	bash tools/trident_ci.sh

# ── Git tags ──────────────────────────────────────────────────────────────────

tag-phase1:
	git add -A
	git commit -m "phase-1.1: guest kernel boots, serial console works (WHP+KVM)"
	git tag phase-1.1

tag-phase2:
	git add -A
	git commit -m "phase-2: SR-IOV BAR0 direct display, DMA-BUF streaming"
	git tag phase-2.0

# ── Housekeeping ──────────────────────────────────────────────────────────────

clean:
	cargo clean
	rm -f guest_kernel
