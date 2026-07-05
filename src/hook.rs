#[derive(Debug)]
pub struct HookHandle;

/// A [`HookHandle`] wrapper which prevents the inner handle from being leaked.
#[derive(Clone, Debug)]
pub struct HookScope<'a>(&'a HookHandle);

impl<'a> HookScope<'a> {
    /// Creates a new [`HookScope`], which immutably borrows the [`HookHandle`],
    /// but does not allow it to be cloned, forgotten or leaked.
    pub fn new(handle: &'a HookHandle) -> Self {
        Self(handle)
    }
}
