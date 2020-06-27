use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;
use core::mem::ManuallyDrop;
use core::ops::Deref;
use core::ptr;
use core::sync::atomic::Ordering;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "stable_deref_trait")]
use stable_deref_trait::{CloneStableDeref, StableDeref};

use super::{ArcBorrow, ArcHandle};

/// An atomically reference counted shared pointer
///
/// See the documentation for [`Arc`] in the standard library.
/// Unlike the standard library `Arc`, this `Arc` holds a pointer to the `T` instead of to the entire `ArcInner`.
/// This makes the struct FFI-compatible, and allows a variety of pointer casts, e.g. `&[Arc<T>]` to `&[&T]`.
///
/// ```text
///  ArcHandle<T>           Arc<T>
///  std::sync::Arc<T>      ArcBorrow<T>
///   |                     |
///   v                     v
///  -----------------------------------
/// | RefCount              | T (data) | [ArcInner<T>]
///  -----------------------------------
/// ```
///
/// This means that this is a direct pointer to
/// its contained data (and can be read from by both C++ and Rust),
/// but we can also convert it to an `ArcHandle<T>` by removing the offset.
///
/// This is very useful if you have an Arc-containing struct shared between Rust and C++,
/// and wish for C++ to be able to read the data behind the `Arc` without incurring
/// an FFI call overhead.
///
/// [`Arc`]: https://doc.rust-lang.org/stable/std/sync/struct.Arc.html
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
        ArcHandle::into_arc(self.clone_handle())
    }
}

impl<T> Drop for Arc<T> {
    fn drop(&mut self) {
        let _ = ArcHandle::from_arc(Arc {
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
    /// Construct an `Arc<T>`
    #[inline]
    pub fn new(data: T) -> Self {
        ArcHandle::into_arc(ArcHandle::new(data))
    }

    /// Temporarily converts `|self|` into a bonafide `ArcHandle` and exposes it to the
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

    /// Makes a mutable reference to the `Arc`, cloning if necessary
    ///
    /// This is functionally equivalent to [`Arc::make_mut`][mm] from the standard library.
    ///
    /// If this `ArcHandle` is uniquely owned, `make_mut()` will provide a mutable
    /// reference to the contents. If not, `make_mut()` will create a _new_ `ArcHandle`
    /// with a copy of the contents, update `this` to point to it, and provide
    /// a mutable reference to its contents.
    ///
    /// This is useful for implementing copy-on-write schemes where you wish to
    /// avoid copying things if your `Arc` is not shared.
    ///
    /// [mm]: https://doc.rust-lang.org/stable/std/sync/struct.Arc.html#method.make_mut
    #[inline]
    pub fn make_mut(&mut self) -> &mut T
    where
        T: Clone,
    {
        unsafe {
            // extract the Arc as an owned variable
            let this = ptr::read(self);
            // treat it as a real Arc
            let mut arc = ArcHandle::from_arc(this);
            // obtain the mutable reference. Cast away the lifetime
            // This may mutate `arc`
            let ret = ArcHandle::make_mut(&mut arc) as *mut _;
            // Store the possibly-mutated arc back inside, after converting
            // it to a Arc again
            ptr::write(self, ArcHandle::into_arc(arc));
            &mut *ret
        }
    }

    /// Clone this `Arc` as an `ArcHandle`
    #[inline]
    pub fn clone_handle(&self) -> ArcHandle<T> {
        Arc::with_handle(self, |a| a.clone())
    }

    /// Produce a pointer to the data that can be converted back
    /// to an `Arc<T>`. This is basically an `&Arc<T>`, without the extra indirection.
    /// It has the benefits of an `&T` but also knows about the underlying refcount
    /// and can be converted into more `Arc<T>`s if necessary.
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

#[cfg(feature = "stable_deref_trait")]
unsafe impl<T> StableDeref for Arc<T> {}
#[cfg(feature = "stable_deref_trait")]
unsafe impl<T> CloneStableDeref for Arc<T> {}

#[cfg(feature = "serde")]
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Arc<T> {
    fn deserialize<D>(deserializer: D) -> Result<Arc<T>, D::Error>
    where
        D: ::serde::de::Deserializer<'de>,
    {
        T::deserialize(deserializer).map(Arc::new)
    }
}

#[cfg(feature = "serde")]
impl<T: Serialize> Serialize for Arc<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::ser::Serializer,
    {
        (**self).serialize(serializer)
    }
}
