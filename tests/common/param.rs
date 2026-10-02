use std::{fmt, hint, ops};

/// Transparent `u32` wrapper without `Drop`.
///
/// Expected to be passed in general purpose registers.
#[derive(Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct GpReg(u32);

/// Transparent `f32` wrapper without `Drop`.
///
/// Expected to be passed in floating point registers.
#[derive(Clone)]
#[repr(transparent)]
pub struct FpReg(f32);

/// Overly-aligned `u32` wrapper *with* `Drop`.
///
/// Expected to be passed on the stack.
#[derive(Clone, PartialEq, Eq)]
#[repr(C, align(16))]
pub struct Stack(u32);

/// Oracle for accumulating a result from the above types.
fn noncommutative_op(a: u32, b: u32) -> u32 {
    a.wrapping_mul(37).wrapping_add(b)
}

/// Base operation for accumulating a result from the above types.
fn pessimized_noncommutative_op(a: u32, b: u32) -> u32 {
    noncommutative_op(hint::black_box(a), hint::black_box(b))
}

impl ops::Add for GpReg {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(pessimized_noncommutative_op(self.0, rhs.0))
    }
}

impl ops::Add for FpReg {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(f32::from_bits(pessimized_noncommutative_op(
            self.0.to_bits(),
            rhs.0.to_bits(),
        )))
    }
}

impl ops::Add for Stack {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(pessimized_noncommutative_op(self.0, rhs.0))
    }
}

impl Default for GpReg {
    fn default() -> Self {
        Self(0xdeadbeef)
    }
}

impl Default for FpReg {
    fn default() -> Self {
        Self(f32::from_bits(0xdeadbeef))
    }
}

impl Default for Stack {
    fn default() -> Self {
        Self(0xdeadbeef)
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        // Pessimize optimizations for this Drop impl.
        hint::black_box(self);
    }
}

impl fmt::Debug for GpReg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Debug for FpReg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.to_bits().fmt(f)
    }
}

impl fmt::Debug for Stack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl PartialEq for FpReg {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for FpReg {}
