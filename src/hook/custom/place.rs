use std::{
    ops::{Deref, DerefMut},
    ptr::NonNull,
    sync::atomic::{AtomicBool, Ordering},
};

use diversion_abi::sync::Mutex;

pub trait Place<T> {
    unsafe fn read(&self) -> T;

    unsafe fn write(&mut self, value: T);
}

pub trait TrivialPlace: Place<Self> + Copy {}

pub trait ResolvePlace<Src> {
    const UNIQUE: Option<usize> = None;

    fn resolve(src: &Src) -> Self;
}

pub trait WithResolved<Src, Args> {
    fn call_with_resolved(&self, src: Src);
}

pub struct FnMutWrapper<F>(Mutex<F>);

pub struct FnOnceWrapper<F> {
    inner: Mutex<Option<F>>,
    flag: AtomicBool,
}

#[repr(transparent)]
pub struct Ref<T, const OFFSET: isize = 0>(*mut T);

#[repr(transparent)]
pub struct UnalignedRef<T, const OFFSET: isize = 0>(*mut T);

impl<F> FnMutWrapper<F> {
    pub(super) fn new(f: F) -> Self {
        Self(Mutex::new(f))
    }
}

impl<F> FnOnceWrapper<F> {
    pub(super) fn new(f: F) -> Self {
        Self {
            inner: Mutex::new(Some(f)),
            flag: AtomicBool::new(true),
        }
    }
}

impl<T> TrivialPlace for T where T: Place<Self> + Copy {}

impl<T: TrivialPlace, const N: usize> Place<Self> for [T; N]
where
    Self: Copy,
{
    #[inline]
    unsafe fn read(&self) -> Self {
        *self
    }

    #[inline]
    unsafe fn write(&mut self, value: Self) {
        *self = value;
    }
}

impl<T, const OFFSET: isize> Deref for Ref<T, OFFSET> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.as_ptr() }
    }
}

impl<T, const OFFSET: isize> DerefMut for Ref<T, OFFSET> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.as_mut_ptr() }
    }
}

macro_rules! impl_trivial_types {
    ($t:ty,) => {
        impl Place<Self> for $t {
            #[inline]
            unsafe fn read(&self) -> Self {
                *self
            }
            #[inline]
            unsafe fn write(&mut self, value: Self) {
                *self = value;
            }
        }
    };
    ($first:ty, $($rest:ty,)+) => {
        impl_trivial_types!($first,);
        impl_trivial_types!($($rest,)+);
    };
}

macro_rules! impl_generic_types {
    ($t:ty,) => {
        impl<T> Place<Self> for $t {
            #[inline]
            unsafe fn read(&self) -> Self {
                *self
            }
            #[inline]
            unsafe fn write(&mut self, value: Self) {
                *self = value;
            }
        }
    };
    ($first:ty, $($rest:ty,)+) => {
        impl_generic_types!($first,);
        impl_generic_types!($($rest,)+);
    };
}

macro_rules! impl_ref_types {
    ($t:ty,) => {
        impl<T, const OFFSET: isize> $t {
            #[inline]
            pub fn as_ptr(&self) -> *const T {
                unsafe { self.0.byte_offset(OFFSET) }
            }
            #[inline]
            pub fn as_mut_ptr(&mut self) -> *mut T {
                unsafe { self.0.byte_offset(OFFSET) }
            }
        }
        impl<T, const OFFSET: isize> Place<Self> for $t {
            #[inline]
            unsafe fn read(&self) -> Self {
                *self
            }
            #[inline]
            unsafe fn write(&mut self, value: Self) {
                *self = value;
            }
        }
        impl<T, const OFFSET: isize> Clone for $t {
            #[inline]
            fn clone(&self) -> Self {
                Self(self.0)
            }
        }
        impl<T, const OFFSET: isize> Copy for $t {}
    };
    ($first:ty, $($rest:ty,)+) => {
        impl_ref_types!($first,);
        impl_ref_types!($($rest,)+);
    };
}

impl_trivial_types! { i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64, (), }

impl_generic_types! { *const T, *mut T, Option<NonNull<T>>, }

impl_ref_types! { Ref<T, OFFSET>, UnalignedRef<T, OFFSET>, }

macro_rules! impl_with_resolved {
    (@impl $($arg:ident: $t:ident,)*) => {
        impl<Fun, Src, $($t,)*> WithResolved<Src, ($($t,)*)> for Fun
        where
            Fun: Fn($($t,)*),
            $($t: ResolvePlace<Src>,)*
        {
            #[inline]
            fn call_with_resolved(&self, _src: Src) {
                const {
                    assert!(!has_dupes(&[$(<$t>::UNIQUE,)*]), "duplicate register access");
                }
                $(let $arg = <$t>::resolve(&_src);)*
                self($($arg,)*);
            }
        }
        impl<Fun, Src, $($t,)*> WithResolved<Src, ($($t,)*)> for FnMutWrapper<Fun>
        where
            Fun: FnMut($($t,)*),
            $($t: ResolvePlace<Src>,)*
        {
            #[inline]
            fn call_with_resolved(&self, _src: Src) {
                const {
                    assert!(!has_dupes(&[$(<$t>::UNIQUE,)*]), "duplicate register access");
                }
                $(let $arg = <$t>::resolve(&_src);)*
                self.0.lock()($($arg,)*);
            }
        }
        impl<Fun, Src, $($t,)*> WithResolved<Src, ($($t,)*)> for FnOnceWrapper<Fun>
        where
            Fun: FnOnce($($t,)*),
            $($t: ResolvePlace<Src>,)*
        {
            #[inline]
            fn call_with_resolved(&self, _src: Src) {
                const {
                    assert!(!has_dupes(&[$(<$t>::UNIQUE,)*]), "duplicate register access");
                }
                $(let $arg = <$t>::resolve(&_src);)*
                if self.flag.load(Ordering::Acquire)
                    && let Some(f) = { self.inner.lock().take() }
                {
                    self.flag.store(false, Ordering::Release);
                    f($($arg,)*);
                }
            }
        }
    };
    ($arg0:ident: $t0:ident, $($arg:ident: $t:ident,)*) => {
        impl_with_resolved!(@impl $arg0: $t0, $($arg: $t,)*);
        impl_with_resolved!($($arg: $t,)*);
    };
    () => {
        impl_with_resolved!(@impl );
    };
}

const fn has_dupes(unique: &[Option<usize>]) -> bool {
    let mut slots = [false; 32];
    let mut i = 0;
    let mut has_dupes = false;
    while i < unique.len() {
        if let Some(unique) = unique[i] {
            has_dupes |= slots[unique];
            slots[unique] = true;
        }
        i += 1;
    }
    has_dupes
}

impl_with_resolved! {
    a: A,
    b: B,
    c: C,
    d: D,
    e: E,
    f: F,
    g: G,
    h: H,
    i: I,
    j: J,
    k: K,
    l: L,
    m: M,
    n: N,
    o: O,
    p: P,
}
