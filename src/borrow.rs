use core::borrow::Borrow;
use core::convert::AsRef;
use core::hash::Hash;
use core::marker::PhantomData;
use core::ops::Deref;
use core::ptr;
use core::sync::atomic::Ordering;
#[cfg(feature = "erasable")]
use erasable::{Erasable, ErasablePtr, ErasedPtr};
#[cfg(feature = "serde")]
use serde::Serialize;
#[cfg(feature = "stable_deref_trait")]
use stable_deref_trait::{CloneStableDeref, StableDeref};

use super::Arc;

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
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(transparent)]
pub struct ArcBorrow<'a, T: ?Sized + 'a> {
    ptr: ptr::NonNull<T>,
    phantom: PhantomData<&'a T>,
}

impl<'a, T: ?Sized> Copy for ArcBorrow<'a, T> {}
impl<'a, T: ?Sized> Clone for ArcBorrow<'a, T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T: ?Sized> ArcBorrow<'a, T> {
    /// Clone this as an `Arc<T>`. This bumps the refcount.
    #[inline]
    pub fn clone_arc(&self) -> Arc<T> {
        self.as_arc().clone()
    }

    /// Borrow this as an `Arc<T>`. This does *not* bump the refcount.
    #[inline]
    pub fn as_arc(&self) -> &Arc<T> {
        unsafe { &*(self as *const ArcBorrow<T> as *const Arc<T>) }
    }

    /// For constructing from a pointer known to be Arc-backed,
    /// e.g. if we obtain such a pointer over FFI
    ///
    /// # Safety
    /// This pointer shouild come from `Arc::into_raw`: this, however, will *not* consume it!
    #[inline]
    pub unsafe fn from_ref(ptr: &'a T) -> Self {
        ArcBorrow::from_raw(ptr)
    }

    /// For constructing from a pointer known to be Arc-backed,
    /// e.g. if we obtain such a pointer over FFI
    ///
    /// # Safety
    /// This pointer shouild come from `Arc::into_raw`: this, however, will *not* consume it!
    #[inline]
    pub unsafe fn from_raw(ptr: *const T) -> Self {
        ArcBorrow {
            ptr: ptr::NonNull::new_unchecked(ptr as *mut T),
            phantom: PhantomData,
        }
    }

    /// Get the internal pointer of an `ArcBorrow`
    #[inline]
    pub fn into_raw(this: Self) -> *const T {
        this.ptr.as_ptr()
    }

    /// Compare two `ArcBorrow`s via pointer equality. Will only return
    /// true if they come from the same allocation
    #[inline]
    pub fn ptr_eq(this: Self, other: Self) -> bool {
        this.ptr == other.ptr
    }

    /// Similar to deref, but uses the lifetime |a| rather than the lifetime of
    /// self, which is incompatible with the signature of the Deref trait.
    #[inline]
    pub fn get(&self) -> &'a T {
        unsafe { &*self.ptr.as_ptr() }
    }

    /// Get the reference count of this `Arc` with a given memory ordering
    pub fn count(this: ArcBorrow<'a, T>, ordering: Ordering) -> usize {
        Arc::count(this.as_arc(), ordering)
    }
}

impl<'a, T: ?Sized> Deref for ArcBorrow<'a, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.get()
    }
}

impl<'a, T: ?Sized> Borrow<Arc<T>> for ArcBorrow<'a, T> {
    fn borrow(&self) -> &Arc<T> {
        self.as_arc()
    }
}

impl<'a, T: ?Sized> Borrow<&'a T> for ArcBorrow<'a, T> {
    fn borrow(&self) -> &&'a T {
        unsafe { &*(self as *const ArcBorrow<T> as *const &T) }
    }
}

impl<'a, T: ?Sized> Borrow<T> for ArcBorrow<'a, T> {
    fn borrow(&self) -> &T {
        self.deref()
    }
}

impl<'a, T: ?Sized> AsRef<Arc<T>> for ArcBorrow<'a, T> {
    fn as_ref(&self) -> &Arc<T> {
        self.as_arc()
    }
}

impl<'a, T: ?Sized> AsRef<&'a T> for ArcBorrow<'a, T> {
    fn as_ref(&self) -> &&'a T {
        unsafe { &*(self as *const ArcBorrow<T> as *const &T) }
    }
}

impl<'a, T: ?Sized> AsRef<T> for ArcBorrow<'a, T> {
    fn as_ref(&self) -> &T {
        self.deref()
    }
}

impl<'a, T: ?Sized> Borrow<*const T> for ArcBorrow<'a, T> {
    #[inline]
    fn borrow(&self) -> &*const T {
        unsafe { &*(self as *const ArcBorrow<T> as *const *const T) }
    }
}

impl<'a, T: ?Sized> AsRef<*const T> for ArcBorrow<'a, T> {
    #[inline]
    fn as_ref(&self) -> &*const T {
        unsafe { &*(self as *const ArcBorrow<T> as *const *const T) }
    }
}

impl<'a, T: ?Sized> Borrow<*mut T> for ArcBorrow<'a, T> {
    #[inline]
    fn borrow(&self) -> &*mut T {
        unsafe { &*(self as *const ArcBorrow<T> as *const *mut T) }
    }
}

impl<'a, T: ?Sized> AsRef<*mut T> for ArcBorrow<'a, T> {
    #[inline]
    fn as_ref(&self) -> &*mut T {
        unsafe { &*(self as *const ArcBorrow<T> as *const *mut T) }
    }
}

impl<'a, T: ?Sized> Borrow<ptr::NonNull<T>> for ArcBorrow<'a, T> {
    #[inline]
    fn borrow(&self) -> &ptr::NonNull<T> {
        unsafe { &*(self as *const ArcBorrow<T> as *const ptr::NonNull<T>) }
    }
}

impl<'a, T: ?Sized> AsRef<ptr::NonNull<T>> for ArcBorrow<'a, T> {
    #[inline]
    fn as_ref(&self) -> &ptr::NonNull<T> {
        unsafe { &*(self as *const ArcBorrow<T> as *const ptr::NonNull<T>) }
    }
}

#[cfg(feature = "stable_deref_trait")]
unsafe impl<'a, T: ?Sized> StableDeref for ArcBorrow<'a, T> {}
#[cfg(feature = "stable_deref_trait")]
unsafe impl<'a, T: ?Sized> CloneStableDeref for ArcBorrow<'a, T> {}

#[cfg(feature = "serde")]
impl<'a, T: ?Sized + Serialize> Serialize for ArcBorrow<'a, T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::ser::Serializer,
    {
        (**self).serialize(serializer)
    }
}

#[cfg(feature = "erasable")]
unsafe impl<'a, T: ?Sized + Erasable> ErasablePtr for ArcBorrow<'a, T> {
    fn erase(this: Self) -> ErasedPtr {
        T::erase(this.ptr)
    }
    unsafe fn unerase(this: ErasedPtr) -> Self {
        ArcBorrow {
            ptr: T::unerase(this),
            phantom: PhantomData,
        }
    }
}
