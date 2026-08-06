use std::ptr::NonNull;

pub trait Place<T> {
    unsafe fn read(&self) -> T;

    unsafe fn write(&mut self, value: T);
}

pub trait TrivialPlace: Place<Self> + Copy {}

pub trait ResolvePlace<Src> {
    const UNIQUE: Option<usize> = None;

    fn resolve(src: &Src) -> Self;
}

pub trait WithResolved<Src, T> {
    fn call_with_resolved(&self, src: Src);
}

#[repr(transparent)]
pub struct Ref<T, const OFFSET: isize = 0>(*mut T);

#[repr(transparent)]
pub struct UnalignedRef<T, const OFFSET: isize = 0>(*mut T);

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

impl<T, U, const OFFSET: isize> Place<T> for Ref<U, OFFSET>
where
    U: Place<T>,
{
    #[inline]
    #[track_caller]
    unsafe fn read(&self) -> T {
        unsafe { (*self.0.byte_offset(OFFSET)).read() }
    }

    #[inline]
    #[track_caller]
    unsafe fn write(&mut self, value: T) {
        unsafe {
            (*self.0.byte_offset(OFFSET)).write(value);
        }
    }
}

impl<T, U, const OFFSET: isize> Place<T> for UnalignedRef<U, OFFSET>
where
    U: Place<T>,
{
    #[inline]
    #[track_caller]
    unsafe fn read(&self) -> T {
        unsafe { self.0.byte_offset(OFFSET).read_unaligned().read() }
    }

    #[inline]
    #[track_caller]
    unsafe fn write(&mut self, value: T) {
        unsafe {
            let ptr = self.0.byte_offset(OFFSET);
            let mut place = ptr.read_unaligned();
            place.write(value);
            ptr.write_unaligned(place);
        }
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

impl_trivial_types! { i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64, (), }

impl_generic_types! { *const T, *mut T, Option<NonNull<T>>, }

macro_rules! impl_with_resolved {
    (@impl $($arg:ident: $t:ident,)*) => {
        impl<Fun, Src, $($t,)*> WithResolved<Src, ($($t,)*)> for Fun
        where
            Fun: Fn($($t,)*),
            $($t: ResolvePlace<Src>,)*
        {
            #[inline(always)]
            fn call_with_resolved(&self, _src: Src) {
                const {
                    let mut slots = [false; 16];
                    let unique: [Option<usize>; _] = [$(<$t>::UNIQUE,)*];
                    let mut i = 0;
                    while i < unique.len() {
                        if let Some(unique) = unique[i] {
                            assert!(!slots[unique], "duplicate register access");
                            slots[unique] = true;
                        }
                        i += 1;
                    }
                }
                $(let $arg = <$t>::resolve(&_src);)*
                self($($arg,)*);
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
}
