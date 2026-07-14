use std::sync::{Mutex as PoisonMutex, PoisonError, RwLock as PoisonRwLock};

pub(crate) mod pod;

#[derive(Debug)]
#[repr(transparent)]
pub struct Mutex<T>(PoisonMutex<T>);

#[derive(Debug)]
#[repr(transparent)]
pub struct RwLock<T>(PoisonRwLock<T>);

pub use std::sync::{MutexGuard, RwLockReadGuard, RwLockWriteGuard};

impl<T> Mutex<T> {
    #[inline]
    pub const fn new(t: T) -> Self {
        Self(PoisonMutex::new(t))
    }

    #[inline]
    pub fn lock(&self) -> MutexGuard<'_, T> {
        self.0.lock().unpoison()
    }
}

impl<T> RwLock<T> {
    #[inline]
    pub const fn new(t: T) -> Self {
        Self(PoisonRwLock::new(t))
    }

    #[inline]
    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        self.0.read().unpoison()
    }

    #[inline]
    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        self.0.write().unpoison()
    }
}

trait PoisonErrorExt<T> {
    fn unpoison(self) -> T;
}

impl<T> PoisonErrorExt<T> for Result<T, PoisonError<T>> {
    #[inline]
    fn unpoison(self) -> T {
        match self {
            Ok(ok) => ok,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

impl<T: Default> Default for Mutex<T> {
    #[inline]
    fn default() -> Self {
        Self::new(Default::default())
    }
}

impl<T: Default> Default for RwLock<T> {
    #[inline]
    fn default() -> Self {
        Self::new(Default::default())
    }
}
