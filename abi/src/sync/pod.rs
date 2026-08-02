#![cfg(feature = "process_ctx")]

use std::{
    hint, mem,
    sync::atomic::{
        AtomicU32,
        Ordering::{Acquire, Relaxed, Release},
    },
};

/// A mutex that is a POD (plain old data) struct.
///
/// It can be zero-initialized in an unlocked state.
#[derive(Debug)]
#[repr(C)]
pub struct PodMutex {
    inner: AtomicU32,
}

/// A spin lock that is a POD (plain old data) struct.
///
/// It can be zero-initialized in an unlocked state.
#[derive(Debug)]
#[repr(C)]
pub struct PodSpinMutex {
    inner: AtomicU32,
}

/// [`PodMutex::lock`] RAII lock guard which unlocks the mutex on drop.
#[derive(Debug)]
#[repr(transparent)]
pub struct MutexGuard<'a>(&'a AtomicU32);

/// [`PodSpinMutex::lock`] RAII lock guard which unlocks the spinlock on drop.
#[derive(Debug)]
#[repr(transparent)]
pub struct SpinMutexGuard<'a>(&'a AtomicU32);

const UNLOCKED: u32 = 0;
const LOCKED: u32 = 1;
const CONTENDED: u32 = 2;

const _: () = assert!(
    unsafe { mem::zeroed::<PodMutex>() }.inner.into_inner() == UNLOCKED,
    "the mutex must zero-initialize in an unlocked state"
);

const _: () = assert!(
    unsafe { mem::zeroed::<PodSpinMutex>() }.inner.into_inner() == UNLOCKED,
    "the mutex must zero-initialize in an unlocked state"
);

impl PodMutex {
    pub fn lock(&self) -> MutexGuard<'_> {
        if let Err(mut state) = self
            .inner
            .compare_exchange(UNLOCKED, LOCKED, Acquire, Relaxed)
        {
            hint::cold_path();

            loop {
                if state != CONTENDED && self.inner.swap(CONTENDED, Acquire) == UNLOCKED {
                    break;
                }

                atomic_wait::wait(&self.inner, CONTENDED);

                state = self.inner.load(Relaxed);
            }
        }

        MutexGuard(&self.inner)
    }
}

impl PodSpinMutex {
    pub fn lock(&self) -> SpinMutexGuard<'_> {
        loop {
            if let Some(guard) = self.try_lock_weak() {
                break guard;
            }

            hint::cold_path();

            while self.inner.load(Relaxed) != UNLOCKED {
                hint::spin_loop();
            }
        }
    }

    fn try_lock_weak(&self) -> Option<SpinMutexGuard<'_>> {
        if self
            .inner
            .compare_exchange_weak(UNLOCKED, LOCKED, Acquire, Relaxed)
            .is_ok()
        {
            Some(SpinMutexGuard(&self.inner))
        } else {
            None
        }
    }
}

impl Drop for MutexGuard<'_> {
    fn drop(&mut self) {
        if self.0.swap(UNLOCKED, Release) == CONTENDED {
            atomic_wait::wake_one(self.0);
        }
    }
}

impl Drop for SpinMutexGuard<'_> {
    fn drop(&mut self) {
        self.0.store(UNLOCKED, Release);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Barrier,
            atomic::{AtomicPtr, AtomicU32, Ordering},
        },
        thread,
    };

    use crate::sync::pod::PodSpinMutex;

    use super::{PodMutex, UNLOCKED};

    impl PodMutex {
        pub const fn new() -> Self {
            Self {
                inner: AtomicU32::new(UNLOCKED),
            }
        }

        pub fn is_locked(&self) -> bool {
            self.inner.load(Ordering::Relaxed) != UNLOCKED
        }
    }

    impl PodSpinMutex {
        pub const fn new() -> Self {
            Self {
                inner: AtomicU32::new(UNLOCKED),
            }
        }

        pub fn is_locked(&self) -> bool {
            self.inner.load(Ordering::Relaxed) != UNLOCKED
        }
    }

    #[test]
    fn lock_mutex() {
        let mutex = PodMutex::new();
        assert!(!mutex.is_locked(), "a newly created mutex must be unlocked");

        let _guard = mutex.lock();
        assert!(mutex.is_locked(), "just locked the mutex");

        drop(_guard);
        assert!(!mutex.is_locked(), "just unlocked the mutex");
    }

    #[test]
    fn lock_spinlock() {
        let mutex = PodSpinMutex::new();
        assert!(!mutex.is_locked(), "a newly created mutex must be unlocked");

        let _guard = mutex.lock();
        assert!(mutex.is_locked(), "just locked the mutex");

        drop(_guard);
        assert!(!mutex.is_locked(), "just unlocked the mutex");
    }

    #[test]
    fn threadpool_count_down() {
        static MUTEX: PodMutex = PodMutex::new();
        do_threadpool_count_down(|| MUTEX.lock());
    }

    #[test]
    fn threadpool_spin_count_down() {
        static MUTEX: PodSpinMutex = PodSpinMutex::new();
        do_threadpool_count_down(|| MUTEX.lock());
    }

    fn do_threadpool_count_down<F, T>(lock: F)
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        const THREADS: usize = 10;

        const BATCHES: usize = 25;
        const BATCH_SIZE: usize = 8;

        let counter = Box::into_raw(Box::new(BATCHES * BATCH_SIZE * THREADS));
        let barrier = Barrier::new(THREADS);

        thread::scope(|s| {
            for _ in 0..THREADS {
                let counter = AtomicPtr::new(counter);
                s.spawn(|| {
                    let counter = counter.into_inner();
                    barrier.wait();

                    for _ in 0..BATCHES {
                        let _guard = lock();
                        let counter = unsafe { &mut *counter };

                        for _ in 0..BATCH_SIZE {
                            *counter -= 1;
                        }
                    }
                });
            }
        });

        let counter = unsafe { *counter };
        assert_eq!(counter, 0);
    }
}
