use crate::shim::sync::atomic::{AtomicBool, Ordering::{AcqRel, Acquire, Release, Relaxed}};
use crate::shim::thread::yield_now;

/// An atomic initializer: mutual exclusion during initialization.
pub struct Init {
    started: AtomicBool,
    done: AtomicBool
}

impl Init {
    /// A ready-to-init initializer.
    #[cfg(not(loom))]
    pub const fn new() -> Init {
        Init {
            started: AtomicBool::new(false),
            done: AtomicBool::new(false)
        }
    }

    /// A ready-to-init initializer.
    #[cfg(loom)]
    pub fn new() -> Init {
        Init {
            started: AtomicBool::new(false),
            done: AtomicBool::new(false)
        }
    }

    /// Returns true if initialization has completed without blocking. If this
    /// function returns false, it may be the case that initialization is
    /// currently in progress. If this function returns `true`, intialization is
    /// guaranteed to be completed.
    #[inline(always)]
    pub fn has_completed(&self) -> bool {
        self.done.load(Acquire)
    }

    /// Mark this initialization as complete, unblocking all threads that may be
    /// waiting. Only the caller that received `true` from `needed()` is
    /// expected to call this method.
    #[inline(always)]
    pub fn mark_complete(&self) {
        // If this is being called from outside of a `needed` block, we need to
        // ensure that `started` is `true` to avoid racing with (return `true`
        // to) future `needed` calls.
        self.started.store(true, Release);
        self.done.store(true, Release);
    }

    /// Blocks until initialization is marked as completed.
    ///
    ///
    // NOTE: Internally, this waits for the the done flag.
    #[inline(always)]
    pub fn wait_until_complete(&self) {
        while !self.done.load(Acquire) { yield_now() }
    }

    #[cold]
    #[inline(always)]
    fn try_to_need_init(&self) -> bool {
        // Quickly check if initialization has already started elsewhere.
        if self.started.load(Relaxed) {
            // If it has, wait until it's finished before returning. Finishing
            // is marked by calling `mark_complete`.
            self.wait_until_complete();
            return false;
        }

        // Try to be the first. If we lose (init_started is true), we wait.
        if self.started.compare_exchange(false, true, AcqRel, Relaxed).is_err() {
            // Another compare_and_swap won. Wait until they're done.
            self.wait_until_complete();
            return false;
        }

        true
    }

    /// If initialization is complete, returns `false`. Otherwise, returns
    /// `true` exactly once, to the caller that should perform initialization.
    /// All other calls block until initialization is marked completed, at which
    /// point `false` is returned.
    ///
    /// If this function is called from multiple threads simulatenously, exactly
    /// one thread is guaranteed to receive `true`. All other threads are
    /// blocked until initialization is marked completed.
    #[inline(always)]
    pub fn needed(&self) -> bool {
        // Quickly check if initialization has finished, and return if so.
        if self.has_completed() {
            return false;
        }

        // We call a different function to attempt the intialiaztion to use
        // Rust's `cold` attribute to try let LLVM know that this is unlikely.
        self.try_to_need_init()
    }
}
