use core::borrow::{Borrow, BorrowMut};
use core::convert::AsRef;
use core::hash::Hash;
use core::ops::{Deref, DerefMut};
#[cfg(feature = "erasable")]
use erasable::{Erasable, ErasablePtr, ErasedPtr};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "stable_deref_trait")]
use stable_deref_trait::StableDeref;

use super::Arc;

/// An `Arc` that is known to be uniquely owned
///
/// When `Arc`s are constructed, they are known to be
/// uniquely owned. In such a case it is safe to mutate
/// the contents of the `Arc`. Normally, one would just handle
/// this by mutating the data on the stack before allocating the
/// `Arc`, however it's possible the data is large or unsized
/// and you need to heap-allocate it earlier in such a way
/// that it can be freely converted into a regular `Arc` once you're
/// done.
///
/// `ArcBox` exists for this purpose, when constructed it performs
/// the same allocations necessary for an `Arc`, however it allows mutable access.
/// Once the mutation is finished, you can call `.shareable()` and get a regular `Arc`
/// out of it. You can also attempt to cast an `Arc` back into a `ArcBox`, which will
/// succeed if the `Arc` is unique
///
/// ```rust
/// # use elysees::ArcBox;
/// # use std::ops::Deref;
/// let data = [1, 2, 3, 4, 5];
/// let mut x = ArcBox::new(data);
/// let x_ptr = x.deref() as *const _;
///
/// x[4] = 7; // mutate!
///
/// // The allocation has been modified, but not moved
/// assert_eq!(x.deref(), &[1, 2, 3, 4, 7]);
/// assert_eq!(x_ptr, x.deref() as *const _);
///
/// let y = x.shareable(); // y is an Arc<T>
///
/// // The allocation has not been modified or moved
/// assert_eq!(y.deref(), &[1, 2, 3, 4, 7]);
/// assert_eq!(x_ptr, y.deref() as *const _);
/// ```

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(transparent)]
pub struct ArcBox<T: ?Sized>(Arc<T>);

impl<T> ArcBox<T> {
    /// Construct a new ArcBox
    #[inline]
    pub fn new(data: T) -> Self {
        ArcBox(Arc::new(data))
    }
}

impl<T: ?Sized> ArcBox<T> {
    /// Convert to a shareable Arc<T> once we're done mutating it
    #[inline]
    pub fn shareable(self) -> Arc<T> {
        self.0
    }
}

impl<T: ?Sized> Deref for ArcBox<T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T {
        &*self.0
    }
}

impl<T: ?Sized> DerefMut for ArcBox<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        // We know this to be uniquely owned
        unsafe { &mut *self.0.ptr.as_ptr() }
    }
}

impl<T: ?Sized> Borrow<T> for ArcBox<T> {
    #[inline]
    fn borrow(&self) -> &T {
        &**self
    }
}

impl<T: ?Sized> AsRef<T> for ArcBox<T> {
    #[inline]
    fn as_ref(&self) -> &T {
        &**self
    }
}

impl<T: ?Sized> BorrowMut<T> for ArcBox<T> {
    #[inline]
    fn borrow_mut(&mut self) -> &mut T {
        &mut **self
    }
}

impl<T: ?Sized> AsMut<T> for ArcBox<T> {
    #[inline]
    fn as_mut(&mut self) -> &mut T {
        &mut **self
    }
}

#[cfg(feature = "stable_deref_trait")]
unsafe impl<T: ?Sized> StableDeref for ArcBox<T> {}

#[cfg(feature = "serde")]
impl<'de, T: Deserialize<'de>> Deserialize<'de> for ArcBox<T> {
    fn deserialize<D>(deserializer: D) -> Result<ArcBox<T>, D::Error>
    where
        D: ::serde::de::Deserializer<'de>,
    {
        T::deserialize(deserializer).map(ArcBox::new)
    }
}

#[cfg(feature = "serde")]
impl<T: ?Sized + Serialize> Serialize for ArcBox<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::ser::Serializer,
    {
        (**self).serialize(serializer)
    }
}

#[cfg(feature = "erasable")]
unsafe impl<T: ?Sized + Erasable> ErasablePtr for ArcBox<T> {
    fn erase(this: Self) -> ErasedPtr {
        ErasablePtr::erase(this.0)
    }

    unsafe fn unerase(this: ErasedPtr) -> Self {
        ArcBox(ErasablePtr::unerase(this))
    }
}
