use std::sync::{
    Mutex as PoisonMutex, MutexGuard, RwLock as PoisonRwLock, RwLockReadGuard, RwLockWriteGuard,
};

#[derive(Debug)]
pub struct Mutex<T> {
    inner: PoisonMutex<T>,
}

#[derive(Debug)]
pub struct RwLock<T> {
    inner: PoisonRwLock<T>,
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

impl<T> RwLock<T> {
    #[inline]
    pub const fn new(t: T) -> Self {
        Self {
            inner: PoisonRwLock::new(t),
        }
    }

    #[inline]
    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        match self.inner.read() {
            Ok(locked) => locked,
            Err(poisoned) => {
                self.inner.clear_poison();
                poisoned.into_inner()
            }
        }
    }

    #[inline]
    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        match self.inner.write() {
            Ok(locked) => locked,
            Err(poisoned) => {
                self.inner.clear_poison();
                poisoned.into_inner()
            }
        }
    }
}

impl<T: Default> Default for RwLock<T> {
    #[inline]
    fn default() -> RwLock<T> {
        RwLock::new(Default::default())
    }
}
