use std::{
    array, fmt,
    panic::{self, AssertUnwindSafe},
    sync::{
        RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

pub mod leak;
pub mod param;
pub mod temp;

pub fn hook_contended<T>(
    hooked: impl Fn() -> T + Send + Sync,
    expected: &RwLock<T>,
    f: impl FnOnce(),
) where
    T: Clone + PartialEq + fmt::Debug + Send + Sync,
{
    let is_finished = AtomicBool::new(false);

    thread::scope(|s| {
        let threads: [_; 4] = array::from_fn(|_| {
            s.spawn(|| {
                while !is_finished.load(Ordering::Acquire) {
                    let expected = expected.try_read();
                    let result = hooked();
                    if let Ok(expected) = expected {
                        assert_eq!(result, *expected);
                    }
                }
            })
        });

        let panicked = panic::catch_unwind(AssertUnwindSafe(f));
        is_finished.store(true, Ordering::Release);

        if let Err(payload) = panicked {
            panic::resume_unwind(payload);
        }

        for thread in threads {
            thread.join().unwrap();
        }
    });
}

macro_rules! tests_impl_recursive {
    ($macro:ident, $cc:literal) => {
        use crate::common::param::{FpReg, GpReg, Stack};
        tests_impl_recursive!($macro, $cc, FpReg);
        tests_impl_recursive!($macro, $cc, GpReg);
        tests_impl_recursive!($macro, $cc, Stack);
    };
    ($macro:ident, $cc:literal, $tys:ty) => {
        tests_impl_recursive!(@ $macro, $cc, $tys, _12, _11, _10, _9, _8, _7, _6, _5, _4, _3, _2, _1, _0,);
    };
    (@ $macro:ident, $cc:literal, $tys:ty, $argn:ident, $($args:ident,)*) => {
        $macro!($cc, $tys, $argn, $($args,)*);
        tests_impl_recursive!(@ $macro, $cc, $tys, $($args,)*);
    };
    (@ $macro:ident, $cc:literal, $tys:ty,) => {};
}

pub(crate) use tests_impl_recursive;
