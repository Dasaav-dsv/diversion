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

/// [`PodMutex::lock`] RAII lock guard which unlocks the mutex on drop.
#[derive(Debug)]
#[repr(transparent)]
pub struct PodMutexGuard<'a>(&'a AtomicU32);

const UNLOCKED: u32 = 0;
const LOCKED: u32 = 1;
const CONTENDED: u32 = 2;

const _: () = assert!(
    unsafe { mem::zeroed::<PodMutex>() }.inner.into_inner() == UNLOCKED,
    "the mutex must zero-initialize in an unlocked state"
);

impl PodMutex {
    #[inline]
    pub fn lock(&self) -> PodMutexGuard<'_> {
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

        PodMutexGuard(&self.inner)
    }
}

impl Drop for PodMutexGuard<'_> {
    #[inline]
    fn drop(&mut self) {
        if self.0.swap(UNLOCKED, Release) == CONTENDED {
            atomic_wait::wake_one(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Barrier,
            atomic::{AtomicU32, Ordering},
        },
        thread,
    };

    use crate::mutex::{PodMutex, UNLOCKED};

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
    fn threadpool_count_down() {
        const THREADS: usize = 10;

        const BATCHES: usize = 25;
        const BATCH_SIZE: usize = 8;

        static MUTEX: PodMutex = PodMutex::new();
        static mut COUNTER: usize = BATCHES * BATCH_SIZE * THREADS;

        let barrier = Barrier::new(THREADS);

        thread::scope(|s| {
            for _ in 0..THREADS {
                s.spawn(|| {
                    barrier.wait();

                    for _ in 0..BATCHES {
                        let _guard = MUTEX.lock();
                        let counter = unsafe { &mut *&raw mut COUNTER };

                        for _ in 0..BATCH_SIZE {
                            *counter -= 1;
                        }
                    }
                });
            }
        });

        let counter = unsafe { (&raw mut COUNTER).read() };
        assert_eq!(counter, 0);
    }
}
