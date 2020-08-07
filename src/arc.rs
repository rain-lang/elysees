use crate::{abort, ArcBorrow, ArcBox};
use alloc::alloc::{alloc, dealloc, Layout};
use core::borrow::Borrow;
use core::cmp::Ordering;
use core::convert::From;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;
use core::mem;
use core::ops::Deref;
use core::ptr;
use core::sync::atomic;
use core::sync::atomic::Ordering::{self as LoadOrdering, Acquire, Relaxed, Release};
use core::{isize, usize};
#[cfg(feature = "erasable")]
use erasable::{Erasable, ErasablePtr, ErasedPtr};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "stable_deref_trait")]
use stable_deref_trait::{CloneStableDeref, StableDeref};

/// A soft limit on the amount of references that may be made to an `Arc`.
///
/// Going above this limit will abort your program (although not
/// necessarily) at _exactly_ `MAX_REFCOUNT + 1` references.
const MAX_REFCOUNT: usize = (isize::MAX) as usize;

/// The object allocated by an Arc<T>
#[repr(C)]
pub struct ArcInner<T: ?Sized> {
    pub(crate) count: atomic::AtomicUsize,
    pub(crate) data: T,
}

impl<T: ?Sized> ArcInner<T> {
    /// Get the theoretical offset of a piece of data in an `ArcInner`, as well as the layout of that `ArcInner`
    #[inline]
    pub fn data_offset(data: &T) -> (Layout, usize) {
        let atomic_layout = Layout::new::<atomic::AtomicUsize>();
        let (layout, offset) = atomic_layout
            .extend(Layout::for_value(data))
            .unwrap_or_else(|_| abort());
        let layout = layout.pad_to_align();
        (layout, offset)
    }
    /// Get an untyped pointer to the inner data from a data pointer, along with a layout
    #[inline]
    pub(crate) unsafe fn inner_ptr(ptr: *const T) -> (Layout, *const u8) {
        let (layout, data_offset) = ArcInner::data_offset(&*ptr);
        (layout, (ptr as *const u8).sub(data_offset))
    }
    /// Get an untyped mutable pointer to the inner data from a data pointer, along with a layout
    #[inline]
    pub(crate) unsafe fn inner_ptr_mut(ptr: *mut T) -> (Layout, *mut u8) {
        let (layout, data_offset) = ArcInner::data_offset(&*ptr);
        (layout, (ptr as *mut u8).sub(data_offset))
    }
    /// Get a reference to the reference count from a data pointer
    #[inline]
    pub(crate) unsafe fn refcount_ptr<'a>(ptr: *const T) -> &'a atomic::AtomicUsize {
        #[allow(clippy::cast_ptr_alignment)]
        &*(ArcInner::inner_ptr(ptr).1 as *const atomic::AtomicUsize)
    }
}

unsafe impl<T: ?Sized + Sync + Send> Send for ArcInner<T> {}
unsafe impl<T: ?Sized + Sync + Send> Sync for ArcInner<T> {}

/// An atomically reference counted shared pointer
///
/// See the documentation for [`Arc`] in the standard library.
/// Unlike the standard library `Arc`, this `Arc` holds a pointer to the `T` instead of to the entire `ArcInner`.
/// This makes the struct FFI-compatible, and allows a variety of pointer casts, e.g. `&[Arc<T>]` to `&[&T]`.
///
/// ```text
///   std::sync::Arc<T>     elysees::Arc<T>
///   |                     |
///   v                     v
///  --------------------------------------
/// | RefCount            | T (data)       | [ArcInner<T>]
///  --------------------------------------
/// ```
///
/// This means that this is a direct pointer to its contained data (and can be read from by both C/C++ and Rust)
///
/// This is very useful if you have an Arc-containing struct shared between Rust and C/C++,
/// and wish for C/C++ to be able to read the data behind the `Arc` without incurring
/// an FFI call overhead. This also enables a variety of useful casts, which are provided as safe functions by
/// the library, e.g. &Arc<T> -> &*const T, which can help with safe implementation of complex `ByAddress`
/// datastructures
///
/// [`Arc`]: https://doc.rust-lang.org/stable/std/sync/struct.Arc.html
#[repr(transparent)]
pub struct Arc<T: ?Sized> {
    pub(crate) ptr: ptr::NonNull<T>,
    pub(crate) phantom: PhantomData<T>,
}

unsafe impl<T: ?Sized + Sync + Send> Send for Arc<T> {}
unsafe impl<T: ?Sized + Sync + Send> Sync for Arc<T> {}

impl<T> Arc<T> {
    /// Construct an `Arc<T>`
    #[inline]
    pub fn new(data: T) -> Self {
        let inner = ArcInner {
            count: atomic::AtomicUsize::new(1),
            data,
        };
        let layout = Layout::for_value(&inner);
        let alloc_ref = unsafe {
            let allocation = alloc(layout) as *mut ArcInner<T>;
            ptr::write(allocation, inner);
            &*allocation
        };
        Arc {
            ptr: (&alloc_ref.data).into(),
            phantom: PhantomData,
        }
    }
}

impl<T: ?Sized> Arc<T> {
    /// Borrow this `Arc<T>` as an `ArcBorrow<T>`
    #[inline]
    pub fn borrow_arc(&self) -> ArcBorrow<T> {
        unsafe { ArcBorrow::from_ref(self.deref()) }
    }
    /// Leak this `Arc<T>`, getting an `ArcBorrow<'static, T>`
    ///
    /// You can call the `get` method on the returned `ArcBorrow` to get an `&'static T`.
    /// Note that using this can (obviously) cause memory leaks!
    #[inline]
    pub fn leak(this: Arc<T>) -> ArcBorrow<'static, T> {
        let result = unsafe { ArcBorrow::from_raw(this.ptr.as_ptr()) };
        mem::forget(this);
        result
    }
    /// Convert the `Arc<T>` to a raw pointer, suitable for use across FFI
    ///
    /// Note: This returns a pointer to the data T, which is offset in the allocation.
    #[inline]
    pub fn into_raw(this: Self) -> *const T {
        let ptr = this.ptr;
        mem::forget(this);
        ptr.as_ptr()
    }
    /// Get the raw pointer underlying this `Arc<T>`
    #[inline]
    pub fn as_ptr(this: &Arc<T>) -> *const T {
        this.ptr.as_ptr()
    }
    /// Convert the `Arc<T>` from a raw pointer obtained from `into_raw()`
    ///
    /// Note: This raw pointer will be offset in the allocation and must be preceded
    /// by the atomic count.
    ///
    /// # Safety
    /// This function must be called with a pointer obtained from `into_raw()`, which
    /// is then invalidated.
    #[inline]
    pub unsafe fn from_raw(ptr: *const T) -> Arc<T> {
        Arc {
            ptr: ptr::NonNull::new_unchecked(ptr as *mut T),
            phantom: PhantomData,
        }
    }
    // Non-inlined part of `drop`. Just invokes the destructor.
    #[inline(never)]
    unsafe fn drop_slow(&mut self) {
        // Step 1: drop data
        ptr::drop_in_place(self.ptr.as_ptr());
        // Step 2: free Inner
        let (layout, data) = ArcInner::inner_ptr_mut(self.ptr.as_ptr());
        dealloc(data, layout)
    }
    /// Get a reference to the reference count of this `Arc`
    #[inline]
    fn borrow_refcount(&self) -> &atomic::AtomicUsize {
        unsafe { ArcInner::refcount_ptr(self.ptr.as_ptr()) }
    }
    /// Whether or not the `Arc` is uniquely owned (is the refcount 1?).
    #[inline]
    pub fn is_unique(&self) -> bool {
        // See the extensive discussion in [1] for why this needs to be Acquire.
        //
        // [1] https://github.com/servo/servo/issues/21186
        Arc::count(self, Acquire) == 1
    }
    /// Try to convert this `Arc` to an `ArcBox` if it is unique
    #[inline]
    pub fn try_unique(this: Self) -> Result<ArcBox<T>, Arc<T>> {
        if this.is_unique() {
            Ok(ArcBox(this))
        } else {
            Err(this)
        }
    }
    /// Get the reference count of this `Arc` with a given ordering
    #[inline]
    pub fn count(this: &Arc<T>, ordering: LoadOrdering) -> usize {
        this.borrow_refcount().load(ordering)
    }
    /// Compare two `Arc`s via pointer equality. Will only return
    /// true if they come from the same allocation
    #[inline]
    pub fn ptr_eq(this: &Self, other: &Self) -> bool {
        this.ptr == other.ptr
    }
}

impl<T: ?Sized> Drop for Arc<T> {
    #[inline]
    fn drop(&mut self) {
        // Because `fetch_sub` is already atomic, we do not need to synchronize
        // with other threads unless we are going to delete the object.
        if self.borrow_refcount().fetch_sub(1, Release) != 1 {
            return;
        }

        // FIXME(bholley): Use the updated comment when [2] is merged.
        //
        // This load is needed to prevent reordering of use of the data and
        // deletion of the data.  Because it is marked `Release`, the decreasing
        // of the reference count synchronizes with this `Acquire` load. This
        // means that use of the data happens before decreasing the reference
        // count, which happens before this load, which happens before the
        // deletion of the data.
        //
        // As explained in the [Boost documentation][1],
        //
        // > It is important to enforce any possible access to the object in one
        // > thread (through an existing reference) to *happen before* deleting
        // > the object in a different thread. This is achieved by a "release"
        // > operation after dropping a reference (any access to the object
        // > through this reference must obviously happened before), and an
        // > "acquire" operation before deleting the object.
        //
        // [1]: (www.boost.org/doc/libs/1_55_0/doc/html/atomic/usage_examples.html)
        // [2]: https://github.com/rust-lang/rust/pull/41714
        self.borrow_refcount().load(Acquire);

        unsafe {
            self.drop_slow();
        }
    }
}

impl<T: ?Sized> Clone for Arc<T> {
    #[inline]
    fn clone(&self) -> Self {
        // Using a relaxed ordering is alright here, as knowledge of the
        // original reference prevents other threads from erroneously deleting
        // the object.
        //
        // As explained in the [Boost documentation][1], Increasing the
        // reference counter can always be done with memory_order_relaxed: New
        // references to an object can only be formed from an existing
        // reference, and passing an existing reference from one thread to
        // another must already provide any required synchronization.
        //
        // [1]: (www.boost.org/doc/libs/1_55_0/doc/html/atomic/usage_examples.html)
        let old_size = self.borrow_refcount().fetch_add(1, Relaxed);

        // However we need to guard against massive refcounts in case someone
        // is `mem::forget`ing Arcs. If we don't do this the count can overflow
        // and users will use-after free. We racily saturate to `isize::MAX` on
        // the assumption that there aren't ~2 billion threads incrementing
        // the reference count at once. This branch will never be taken in
        // any realistic program.
        //
        // We abort because such a program is incredibly degenerate, and we
        // don't care to support it.
        if old_size > MAX_REFCOUNT {
            abort();
        }

        Arc {
            ptr: self.ptr,
            phantom: PhantomData,
        }
    }
}

impl<T: ?Sized> Deref for Arc<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        unsafe { &*self.ptr.as_ptr() }
    }
}

impl<T: Clone> Arc<T> {
    /// Makes a mutable reference to the `ArcHandle`, cloning if necessary
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
    pub fn make_mut(this: &mut Self) -> &mut T {
        if !this.is_unique() {
            // Another pointer exists; clone
            *this = Arc::new((**this).clone());
        }

        unsafe {
            // This unsafety is ok because we're guaranteed that the pointer
            // returned is the *only* pointer that will ever be returned to T. Our
            // reference count is guaranteed to be 1 at this point, and we required
            // the Arc itself to be `mut`, so we're returning the only possible
            // reference to the inner data.
            &mut *this.ptr.as_ptr()
        }
    }
    /// Convert this `Arc` to an `ArcBox`, cloning the internal data if necessary for uniqueness
    #[inline]
    pub fn unique(this: Self) -> ArcBox<T> {
        if this.is_unique() {
            ArcBox(this)
        } else {
            ArcBox::new(this.deref().clone())
        }
    }
}

impl<T: ?Sized + PartialEq> PartialEq for Arc<T> {
    fn eq(&self, other: &Arc<T>) -> bool {
        *(*self) == *(*other)
    }
    #[allow(clippy::partialeq_ne_impl)]
    fn ne(&self, other: &Arc<T>) -> bool {
        *(*self) != *(*other)
    }
}

impl<T: ?Sized + PartialOrd> PartialOrd for Arc<T> {
    fn partial_cmp(&self, other: &Arc<T>) -> Option<Ordering> {
        (**self).partial_cmp(&**other)
    }

    fn lt(&self, other: &Arc<T>) -> bool {
        *(*self) < *(*other)
    }

    fn le(&self, other: &Arc<T>) -> bool {
        *(*self) <= *(*other)
    }

    fn gt(&self, other: &Arc<T>) -> bool {
        *(*self) > *(*other)
    }

    fn ge(&self, other: &Arc<T>) -> bool {
        *(*self) >= *(*other)
    }
}

impl<T: ?Sized + Ord> Ord for Arc<T> {
    fn cmp(&self, other: &Arc<T>) -> Ordering {
        (**self).cmp(&**other)
    }
}

impl<T: ?Sized + Eq> Eq for Arc<T> {}

impl<T: ?Sized + fmt::Display> fmt::Display for Arc<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for Arc<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized> fmt::Pointer for Arc<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Pointer::fmt(&Arc::as_ptr(self), f)
    }
}

impl<T: Default> Default for Arc<T> {
    #[inline]
    fn default() -> Arc<T> {
        Arc::new(Default::default())
    }
}

impl<T: ?Sized + Hash> Hash for Arc<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state)
    }
}

impl<T> From<T> for Arc<T> {
    #[inline]
    fn from(t: T) -> Self {
        Arc::new(t)
    }
}

impl<T: ?Sized> Borrow<T> for Arc<T> {
    #[inline]
    fn borrow(&self) -> &T {
        &**self
    }
}

impl<T: ?Sized> AsRef<T> for Arc<T> {
    #[inline]
    fn as_ref(&self) -> &T {
        &**self
    }
}

impl<T: ?Sized> AsRef<*const T> for Arc<T> {
    #[inline]
    fn as_ref(&self) -> &*const T {
        unsafe { &*(self as *const Arc<T> as *const *const T) }
    }
}

impl<T: ?Sized> AsRef<*mut T> for Arc<T> {
    #[inline]
    fn as_ref(&self) -> &*mut T {
        unsafe { &*(self as *const Arc<T> as *const *mut T) }
    }
}

impl<T: ?Sized> AsRef<ptr::NonNull<T>> for Arc<T> {
    #[inline]
    fn as_ref(&self) -> &ptr::NonNull<T> {
        unsafe { &*(self as *const Arc<T> as *const ptr::NonNull<T>) }
    }
}

#[cfg(feature = "stable_deref_trait")]
unsafe impl<T: ?Sized> StableDeref for Arc<T> {}
#[cfg(feature = "stable_deref_trait")]
unsafe impl<T: ?Sized> CloneStableDeref for Arc<T> {}

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
impl<T: ?Sized + Serialize> Serialize for Arc<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::ser::Serializer,
    {
        (**self).serialize(serializer)
    }
}

#[cfg(feature = "erasable")]
unsafe impl<T: ?Sized + Erasable> ErasablePtr for Arc<T> {
    fn erase(this: Self) -> ErasedPtr {
        let ptr = unsafe { ptr::NonNull::new_unchecked(Arc::into_raw(this) as *mut _) };
        T::erase(ptr)
    }

    unsafe fn unerase(this: ErasedPtr) -> Self {
        Self::from_raw(T::unerase(this).as_ptr())
    }
}

#[cfg(feature = "slice-dst")]
mod slice_dst_impl {
    use super::*;
    use slice_dst::{AllocSliceDst, SliceDst, TryAllocSliceDst};

    unsafe impl<S: ?Sized + SliceDst> TryAllocSliceDst<S> for Arc<S> {
        unsafe fn try_new_slice_dst<I, E>(len: usize, init: I) -> Result<Self, E>
        where
            I: FnOnce(ptr::NonNull<S>) -> Result<(), E>,
        {
            pub struct RawAlloc(*mut u8, Layout);

            impl Drop for RawAlloc {
                fn drop(&mut self) {
                    unsafe { dealloc(self.0, self.1) }
                }
            }

            // Compute layouts
            let slice_layout = S::layout_for(len);
            let count_layout = Layout::new::<atomic::AtomicUsize>();
            let (inner_layout, slice_offset) = count_layout
                .extend(slice_layout)
                .expect("Integer overflow computing slice layout");
            // Allocate
            let inner_alloc = alloc(inner_layout);
            let drop_guard = RawAlloc(inner_alloc, inner_layout);
            {
                #[allow(clippy::cast_ptr_alignment)]
                // Write counter
                ptr::write(
                    inner_alloc as *mut atomic::AtomicUsize,
                    atomic::AtomicUsize::new(1),
                );
            }

            // Get slice pointer
            let slice_addr = inner_alloc.add(slice_offset) as *mut ();
            let slice_ptr = core::slice::from_raw_parts_mut(slice_addr, len);

            // Get DST pointer
            let ptr = S::retype(ptr::NonNull::new_unchecked(slice_ptr));

            // Attempt to initialize the DST pointer
            init(ptr)?;

            // Successful construction: forget the drop guard and make an `Arc`
            mem::forget(drop_guard);
            Ok(Arc {
                ptr,
                phantom: PhantomData,
            })
        }
    }

    unsafe impl<S: ?Sized + SliceDst> AllocSliceDst<S> for Arc<S> {
        unsafe fn new_slice_dst<I>(len: usize, init: I) -> Self
        where
            I: FnOnce(ptr::NonNull<S>),
        {
            enum Void {} // or never (!) once it is stable
            #[allow(clippy::unit_arg)]
            let init = |ptr| Ok::<(), Void>(init(ptr));
            match Self::try_new_slice_dst(len, init) {
                Ok(a) => a,
                Err(void) => match void {},
            }
        }
    }
}

#[cfg(feature = "arbitrary")]
mod arbitrary_impl {
    use super::*;
    use arbitrary::{Arbitrary, Result, Unstructured};
    impl<T: Arbitrary> Arbitrary for Arc<T> {
        fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
            T::arbitrary(u).map(Arc::new)
        }
        fn arbitrary_take_rest(u: Unstructured<'_>) -> Result<Self> {
            T::arbitrary_take_rest(u).map(Arc::new)
        }
        fn size_hint(depth: usize) -> (usize, Option<usize>) {
            T::size_hint(depth)
        }
        fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
            Box::new(self.deref().shrink().map(Arc::new))
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "std")]
    #[test]
    fn data_offset_sanity_tests() {
        use super::*;
        #[allow(dead_code)]
        struct MyStruct {
            id: usize,
            name: String,
            hash: u64,
        };
        let inner = ArcInner {
            count: atomic::AtomicUsize::new(1),
            data: MyStruct {
                id: 596843,
                name: "Jane".into(),
                hash: 0xFF45345,
            },
        };
        let data = &inner.data;
        let data_ptr = data as *const _;
        let data_addr = data_ptr as usize;
        let inner_addr = &inner as *const _ as usize;
        let (layout, data_offset) = ArcInner::data_offset(data);
        assert_eq!(data_addr - inner_addr, data_offset);
        assert_eq!(layout, Layout::for_value(&inner));
    }
}
