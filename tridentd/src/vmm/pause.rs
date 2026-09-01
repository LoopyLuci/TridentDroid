//! Shared pause/resume + register-capture protocol for stopping every vCPU
//! thread of a running VM cleanly — needed by snapshotting.
//!
//! Deliberately backend-agnostic and requires no `Hypervisor` trait changes:
//! it works by having the PIO/MMIO access callback (which fires on
//! essentially every guest instruction that touches an I/O port or device —
//! very frequently for any real workload) bail out with a sentinel error
//! when a pause has been requested. That causes `Hypervisor::run_vcpu` to
//! return control to the vCPU thread's own loop (`vcpu_loop.rs`), which
//! still holds `&vcpu`/`hyp` at that point and can capture register state,
//! then parks until told to resume.
//!
//! Known limitation, accepted rather than solved here: a vCPU executing an
//! arbitrarily long stretch with *zero* PIO/MMIO accesses (a pure ALU tight
//! loop) won't be interrupted until its next access — not a concern for any
//! realistic guest workload (timer/UART/virtio polling all generate
//! frequent accesses), and not worth the complexity of per-backend
//! cross-thread cancellation (`WHvCancelRunVirtualProcessor` / KVM
//! `immediate_exit` + signal delivery) for a case that doesn't arise here.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use trident_hal::{Regs, Sregs};

/// Sentinel error used to unwind out of `run_vcpu` when a pause has been
/// requested — distinguishable from a genuine hypervisor error via
/// [`is_pause_requested`].
#[derive(Debug)]
pub struct PauseRequested;

impl std::fmt::Display for PauseRequested {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "vCPU pause requested")
    }
}

impl std::error::Error for PauseRequested {}

pub fn is_pause_requested(err: &anyhow::Error) -> bool {
    err.downcast_ref::<PauseRequested>().is_some()
}

/// One vCPU's captured state, recorded when it parks in response to a pause
/// request.
pub struct ParkedVcpu {
    pub regs: Regs,
    pub sregs: Sregs,
}

struct GateState {
    /// Filled in by each vCPU thread once parked; drained by the
    /// coordinator once every slot is `Some`.
    parked: Vec<Option<ParkedVcpu>>,
    resume: bool,
}

/// Shared coordination point between a snapshot coordinator and every vCPU
/// thread of one VM. One `PauseGate` per `Vm`.
pub struct PauseGate {
    request: AtomicBool,
    inner: Mutex<GateState>,
    cond: Condvar,
}

impl PauseGate {
    pub fn new(vcpu_count: usize) -> Arc<Self> {
        let mut parked = Vec::with_capacity(vcpu_count);
        parked.resize_with(vcpu_count, || None);
        Arc::new(Self {
            request: AtomicBool::new(false),
            inner: Mutex::new(GateState { parked, resume: true }),
            cond: Condvar::new(),
        })
    }

    /// Checked by the PIO/MMIO access callback on every access.
    pub fn should_pause(&self) -> bool {
        self.request.load(Ordering::SeqCst)
    }

    pub fn vcpu_count(&self) -> usize {
        self.inner.lock().unwrap().parked.len()
    }

    /// Called by a vCPU thread once it has captured its own register state
    /// in response to a pause request. Blocks until the coordinator
    /// resumes everyone.
    pub fn park(&self, index: usize, state: ParkedVcpu) {
        let mut guard = self.inner.lock().unwrap();
        guard.parked[index] = Some(state);
        self.cond.notify_all();
        while !guard.resume {
            guard = self.cond.wait(guard).unwrap();
        }
    }

    /// Coordinator side: request a pause, wait for every vCPU to park, run
    /// `f` with all captured states, then resume everyone. `f` runs with no
    /// lock held, so it's safe to do slow I/O (writing a snapshot file) in it.
    pub fn pause_and<R>(&self, f: impl FnOnce(&[ParkedVcpu]) -> R) -> R {
        self.request.store(true, Ordering::SeqCst);

        let states: Vec<ParkedVcpu> = {
            let mut guard = self.inner.lock().unwrap();
            guard.resume = false;
            while guard.parked.iter().any(|p| p.is_none()) {
                guard = self.cond.wait(guard).unwrap();
            }
            guard.parked.iter_mut().map(|p| p.take().unwrap()).collect()
        };

        let result = f(&states);

        {
            let mut guard = self.inner.lock().unwrap();
            guard.resume = true;
        }
        self.request.store(false, Ordering::SeqCst);
        self.cond.notify_all();

        result
    }
}
