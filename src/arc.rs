use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;
use core::mem::ManuallyDrop;
use core::ops::Deref;
use core::ptr;
use core::sync::atomic::Ordering;

use super::{ArcBorrow, ArcHandle};

/// An `Arc`, except it holds a pointer to the T instead of to the
/// entire ArcInner. This struct is FFI-compatible.
///
/// ```text
///  ArcHandle<T>   Arc<T>
///   |              |
///   v              v
///  ----------------------------
/// | RefCount       | T (data) | [ArcInner<T>]
///  ----------------------------
/// ```
///
/// This means that this is a direct pointer to
/// its contained data (and can be read from by both C++ and Rust),
/// but we can also convert it to a "regular" Arc<T> by removing the offset.
///
/// This is very useful if you have an Arc-containing struct shared between Rust and C++,
/// and wish for C++ to be able to read the data behind the `Arc` without incurring
/// an FFI call overhead.
#[derive(Eq)]
#[repr(transparent)]
pub struct Arc<T> {
    pub(crate) ptr: ptr::NonNull<T>,
    pub(crate) phantom: PhantomData<T>,
}

unsafe impl<T: Sync + Send> Send for Arc<T> {}
unsafe impl<T: Sync + Send> Sync for Arc<T> {}

impl<T> Deref for Arc<T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.ptr.as_ptr() }
    }
}

impl<T> Clone for Arc<T> {
    #[inline]
    fn clone(&self) -> Self {
        ArcHandle::into_raw_offset(self.clone_handle())
    }
}

impl<T> Drop for Arc<T> {
    fn drop(&mut self) {
        let _ = ArcHandle::from_raw_offset(Arc {
            ptr: self.ptr.clone(),
            phantom: PhantomData,
        });
    }
}

impl<T: fmt::Debug> fmt::Debug for Arc<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: PartialEq> PartialEq for Arc<T> {
    fn eq(&self, other: &Arc<T>) -> bool {
        *(*self) == *(*other)
    }

    fn ne(&self, other: &Arc<T>) -> bool {
        *(*self) != *(*other)
    }
}

impl<T: Hash> Hash for Arc<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state)
    }
}

impl<T> Arc<T> {
    /// Temporarily converts |self| into a bonafide Arc and exposes it to the
    /// provided callback. The refcount is not modified.
    #[inline]
    pub fn with_handle<F, U>(&self, f: F) -> U
    where
        F: FnOnce(&ArcHandle<T>) -> U,
    {
        // Synthesize transient Arc, which never touches the refcount of the ArcInner.
        let transient = unsafe { ManuallyDrop::new(ArcHandle::from_raw(self.ptr.as_ptr())) };

        // Expose the transient Arc to the callback, which may clone it if it wants.
        let result = f(&transient);

        // Forward the result.
        result
    }

    /// If uniquely owned, provide a mutable reference
    /// Else create a copy, and mutate that
    ///
    /// This is functionally the same thing as `Arc::make_mut`
    #[inline]
    pub fn make_mut(&mut self) -> &mut T
    where
        T: Clone,
    {
        unsafe {
            // extract the Arc as an owned variable
            let this = ptr::read(self);
            // treat it as a real Arc
            let mut arc = ArcHandle::from_raw_offset(this);
            // obtain the mutable reference. Cast away the lifetime
            // This may mutate `arc`
            let ret = ArcHandle::make_mut(&mut arc) as *mut _;
            // Store the possibly-mutated arc back inside, after converting
            // it to a Arc again
            ptr::write(self, ArcHandle::into_raw_offset(arc));
            &mut *ret
        }
    }

    /// Clone it as an `ArcHandle`
    #[inline]
    pub fn clone_handle(&self) -> ArcHandle<T> {
        Arc::with_handle(self, |a| a.clone())
    }

    /// Produce a pointer to the data that can be converted back
    /// to an `Arc`
    #[inline]
    pub fn borrow_arc<'a>(&'a self) -> ArcBorrow<'a, T> {
        ArcBorrow(&**self)
    }

    /// Get the reference count of this `Arc` with a given memory ordering
    #[inline]
    pub fn get_count(&self, ordering: Ordering) -> usize {
        self.with_handle(|a| a.get_count(ordering))
    }
}
