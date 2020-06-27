use crate::abort;
use alloc::alloc::{alloc, dealloc, Layout};
use core::marker::PhantomData;
use core::mem;
use core::ptr;
use core::sync::atomic::{self, Ordering::*};

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
    pub fn data_offset(data: &T) -> (Layout, usize) {
        let atomic_layout = Layout::new::<atomic::AtomicUsize>();
        atomic_layout
            .extend(Layout::for_value(data))
            .unwrap_or_else(|_| abort())
    }
    /// Get an untyped pointer to the inner data from a data pointer, along with a layout
    pub(crate) unsafe fn inner_ptr<'a>(ptr: *const T) -> (Layout, *const u8) {
        let (layout, data_offset) = ArcInner::data_offset(&*ptr);
        (layout, (ptr as *const u8).sub(data_offset))
    }
    /// Get an untyped mutable pointer to the inner data from a data pointer, along with a layout
    pub(crate) unsafe fn inner_ptr_mut<'a>(ptr: *mut T) -> (Layout, *mut u8) {
        let (layout, data_offset) = ArcInner::data_offset(&*ptr);
        (layout, (ptr as *mut u8).sub(data_offset))
    }
    /// Get a reference to the reference count from a data pointer
    pub(crate) unsafe fn refcount_ptr<'a>(ptr: *const T) -> &'a atomic::AtomicUsize {
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
            count: atomic::AtomicUsize::new(0),
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
    /// Convert the `Arc<T>` to a raw pointer, suitable for use across FFI
    ///
    /// Note: This returns a pointer to the data T, which is offset in the allocation.
    #[inline]
    pub fn into_raw(this: Self) -> *const T {
        let ptr = this.ptr;
        mem::forget(this);
        ptr.as_ptr()
    }
    /// Convert the `Arc<T>` from a raw pointer obtained from `into_raw()`
    ///
    /// Note: This raw pointer will be offset in the allocation and must be preceded
    /// by the atomic count.
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
        unsafe {
            ArcInner::refcount_ptr(self.ptr.as_ptr())
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn data_offset_sanity_tests() {
        #[allow(dead_code)]
        struct MyStruct {
            id: usize,
            name: String,
            hash: u64,
        };
        let inner = ArcInner {
            count: atomic::AtomicUsize::new(0),
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
