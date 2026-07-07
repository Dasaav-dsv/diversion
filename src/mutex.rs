use std::sync::{Mutex as PoisonMutex, MutexGuard};

#[derive(Debug)]
pub struct Mutex<T> {
    inner: PoisonMutex<T>,
}

impl<T> Mutex<T> {
    #[inline]
    pub const fn new(t: T) -> Self {
        Self {
            inner: PoisonMutex::new(t),
        }
    }

    #[inline]
    pub fn lock(&self) -> MutexGuard<'_, T> {
        match self.inner.lock() {
            Ok(locked) => locked,
            Err(poisoned) => {
                self.inner.clear_poison();
                poisoned.into_inner()
            }
        }
    }
}

impl<T: Default> Default for Mutex<T> {
    #[inline]
    fn default() -> Mutex<T> {
        Mutex::new(Default::default())
    }
}
