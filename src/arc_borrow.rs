use core::hash::Hash;
use core::mem;
use core::mem::ManuallyDrop;
use core::ops::Deref;
use core::sync::atomic::Ordering;

use super::{Arc, ArcHandle};

/// A "borrowed `Arc`". This is a pointer to
/// a T that is known to have been allocated within an
/// `Arc`.
///
/// This is equivalent in guarantees to `&ArcHandle<T>`, however it is
/// a bit more flexible. To obtain an `&ArcHandle<T>` you must have
/// an `ArcHandle<T>` instance somewhere pinned down until we're done with it.
/// It's also a direct pointer to `T`, so using this involves less pointer-chasing
///
/// However, C++ code may hand us refcounted things as pointers to T directly,
/// so we have to conjure up a temporary `Arc` on the stack each time. The
/// same happens for when the object is managed by a `Arc`.
///
/// `ArcBorrow` lets us deal with borrows of known-refcounted objects
/// without needing to worry about where the `Arc<T>` is.
#[derive(Debug, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub struct ArcBorrow<'a, T: 'a>(pub(crate) &'a T);

impl<'a, T> Copy for ArcBorrow<'a, T> {}
impl<'a, T> Clone for ArcBorrow<'a, T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T> ArcBorrow<'a, T> {
    /// Clone this as an `ArcHandle<T>`. This bumps the refcount.
    #[inline]
    pub fn clone_handle(&self) -> ArcHandle<T> {
        let arc = unsafe { ArcHandle::from_raw(self.0) };
        // addref it!
        mem::forget(arc.clone());
        arc
    }

    /// Clone this as an `Arc<T>`. This bumps the refcount.
    #[inline]
    pub fn clone_arc(&self) -> Arc<T> {
        ArcHandle::into_raw_offset(self.clone_handle())
    }

    /// For constructing from a reference known to be Arc-backed,
    /// e.g. if we obtain such a reference over FFI
    #[inline]
    pub unsafe fn from_ref(r: &'a T) -> Self {
        ArcBorrow(r)
    }

    /// Compare two `ArcBorrow`s via pointer equality. Will only return
    /// true if they come from the same allocation
    #[inline]
    pub fn ptr_eq(this: &Self, other: &Self) -> bool {
        this.0 as *const T == other.0 as *const T
    }

    /// Temporarily converts |self| into a bonafide Arc and exposes it to the
    /// provided callback. The refcount is not modified.
    #[inline]
    pub fn with_arc<F, U>(&self, f: F) -> U
    where
        F: FnOnce(&ArcHandle<T>) -> U,
        T: 'static,
    {
        // Synthesize transient Arc, which never touches the refcount.
        let transient = unsafe { ManuallyDrop::new(ArcHandle::from_raw(self.0)) };

        // Expose the transient Arc to the callback, which may clone it if it wants.
        let result = f(&transient);

        // Forward the result.
        result
    }

    /// Similar to deref, but uses the lifetime |a| rather than the lifetime of
    /// self, which is incompatible with the signature of the Deref trait.
    #[inline]
    pub fn get(&self) -> &'a T {
        self.0
    }
    /// Get the reference count of this `Arc` with a given memory ordering
    pub fn get_count(&self, ordering: Ordering) -> usize
    where
        T: 'static,
    {
        self.with_arc(|a| a.get_count(ordering))
    }
}

impl<'a, T> Deref for ArcBorrow<'a, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.0
    }
}
