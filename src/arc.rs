use alloc::alloc::{alloc, dealloc, Layout};
use core::marker::PhantomData;
use core::mem;
use core::ptr;
use core::sync::atomic;

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
    /// Get the theoretical offset of a piece of data in an `ArcInner`
    pub fn data_offset(data: &T) -> usize {
        let count_size = std::mem::size_of::<atomic::AtomicUsize>();
        let data_alignment = std::mem::align_of_val(data);
        let data_offset = ((count_size + data_alignment - 1) / data_alignment) * data_alignment;
        data_offset
    }
    /// Get a reference to the reference count from a data pointer
    pub(crate) unsafe fn refcount_ptr<'a>(ptr: *const T) -> &'a atomic::AtomicUsize {
        let data_offset = ArcInner::data_offset(&*ptr);
        let count_ptr = (ptr as *const u8).sub(data_offset) as *const atomic::AtomicUsize;
        &(*count_ptr)
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
            hash: u64
        };
        let inner = ArcInner {
            count: atomic::AtomicUsize::new(0),
            data: MyStruct {
                id: 596843,
                name: "Jane".into(),
                hash: 0xFF45345
            }
        };
        let data = &inner.data;
        let data_ptr = data as *const _;
        let data_addr = data_ptr as usize;
        let inner_addr = &inner as *const _ as usize;
        assert_eq!(
            data_addr - inner_addr,
            ArcInner::data_offset(data)
        )
    }
}