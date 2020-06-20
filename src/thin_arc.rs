use core::ffi::c_void;
use core::hash::{Hash, Hasher};
use core::iter::{ExactSizeIterator, Iterator};
use core::marker::PhantomData;
use core::mem;
use core::mem::ManuallyDrop;
use core::ops::Deref;
use core::ptr;
use core::slice;
use core::usize;

use super::{Arc, ArcInner, HeaderSliceWithLength, HeaderWithLength};

/// A "thin" `Arc` containing dynamically sized data
///
/// This is functionally equivalent to `Arc<(H, [T])>`
///
/// When you create an `Arc` containing a dynamically sized type
/// like `HeaderSlice<H, [T]>`, the `Arc` is represented on the stack
/// as a "fat pointer", where the length of the slice is stored
/// alongside the `Arc`'s pointer. In some situations you may wish to
/// have a thin pointer instead, perhaps for FFI compatibility
/// or space efficiency.
///
/// Note that we use `[T; 0]` in order to have the right alignment for `T`.
///
/// `ThinArc` solves this by storing the length in the allocation itself,
/// via `HeaderSliceWithLength`.
#[repr(transparent)]
pub struct ThinArc<H, T> {
    ptr: ptr::NonNull<ArcInner<HeaderSliceWithLength<H, [T; 0]>>>,
    phantom: PhantomData<(H, T)>,
}

unsafe impl<H: Sync + Send, T: Sync + Send> Send for ThinArc<H, T> {}
unsafe impl<H: Sync + Send, T: Sync + Send> Sync for ThinArc<H, T> {}

// Synthesize a fat pointer from a thin pointer.
//
// See the comment around the analogous operation in from_header_and_iter.
fn thin_to_thick<H, T>(
    thin: *mut ArcInner<HeaderSliceWithLength<H, [T; 0]>>,
) -> *mut ArcInner<HeaderSliceWithLength<H, [T]>> {
    let len = unsafe { (*thin).data.header.length };
    let fake_slice: *mut [T] = unsafe { slice::from_raw_parts_mut(thin as *mut T, len) };

    fake_slice as *mut ArcInner<HeaderSliceWithLength<H, [T]>>
}

impl<H, T> ThinArc<H, T> {
    /// Temporarily converts |self| into a bonafide Arc and exposes it to the
    /// provided callback. The refcount is not modified.
    #[inline]
    pub fn with_arc<F, U>(&self, f: F) -> U
    where
        F: FnOnce(&Arc<HeaderSliceWithLength<H, [T]>>) -> U,
    {
        // Synthesize transient Arc, which never touches the refcount of the ArcInner.
        let transient = unsafe {
            ManuallyDrop::new(Arc {
                p: ptr::NonNull::new_unchecked(thin_to_thick(self.ptr.as_ptr())),
                phantom: PhantomData,
            })
        };

        // Expose the transient Arc to the callback, which may clone it if it wants.
        let result = f(&transient);

        // Forward the result.
        result
    }

    /// Creates a `ThinArc` for a HeaderSlice using the given header struct and
    /// iterator to generate the slice.
    pub fn from_header_and_iter<I>(header: H, items: I) -> Self
    where
        I: Iterator<Item = T> + ExactSizeIterator,
    {
        let header = HeaderWithLength::new(header, items.len());
        Arc::into_thin(Arc::from_header_and_iter(header, items))
    }

    /// Returns the address on the heap of the ThinArc itself -- not the T
    /// within it -- for memory reporting.
    #[inline]
    pub fn ptr(&self) -> *const c_void {
        self.ptr.as_ptr() as *const ArcInner<T> as *const c_void
    }

    /// Returns the address on the heap of the Arc itself -- not the T within it -- for memory
    /// reporting.
    #[inline]
    pub fn heap_ptr(&self) -> *const c_void {
        self.ptr()
    }
}

impl<H, T> Deref for ThinArc<H, T> {
    type Target = HeaderSliceWithLength<H, [T]>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { &(*thin_to_thick(self.ptr.as_ptr())).data }
    }
}

impl<H, T> Clone for ThinArc<H, T> {
    #[inline]
    fn clone(&self) -> Self {
        ThinArc::with_arc(self, |a| Arc::into_thin(a.clone()))
    }
}

impl<H, T> Drop for ThinArc<H, T> {
    #[inline]
    fn drop(&mut self) {
        let _ = Arc::from_thin(ThinArc {
            ptr: self.ptr,
            phantom: PhantomData,
        });
    }
}

impl<H, T> Arc<HeaderSliceWithLength<H, [T]>> {
    /// Converts an `Arc` into a `ThinArc`. This consumes the `Arc`, so the refcount
    /// is not modified.
    #[inline]
    pub fn into_thin(a: Self) -> ThinArc<H, T> {
        assert_eq!(
            a.header.length,
            a.slice.len(),
            "Length needs to be correct for ThinArc to work"
        );
        let fat_ptr: *mut ArcInner<HeaderSliceWithLength<H, [T]>> = a.ptr();
        mem::forget(a);
        let thin_ptr = fat_ptr as *mut [usize] as *mut usize;
        ThinArc {
            ptr: unsafe {
                ptr::NonNull::new_unchecked(
                    thin_ptr as *mut ArcInner<HeaderSliceWithLength<H, [T; 0]>>,
                )
            },
            phantom: PhantomData,
        }
    }

    /// Converts a `ThinArc` into an `Arc`. This consumes the `ThinArc`, so the refcount
    /// is not modified.
    #[inline]
    pub fn from_thin(a: ThinArc<H, T>) -> Self {
        let ptr = thin_to_thick(a.ptr.as_ptr());
        mem::forget(a);
        unsafe {
            Arc {
                p: ptr::NonNull::new_unchecked(ptr),
                phantom: PhantomData,
            }
        }
    }
}

impl<H: PartialEq, T: PartialEq> PartialEq for ThinArc<H, T> {
    #[inline]
    fn eq(&self, other: &ThinArc<H, T>) -> bool {
        ThinArc::with_arc(self, |a| ThinArc::with_arc(other, |b| *a == *b))
    }
}

impl<H: Eq, T: Eq> Eq for ThinArc<H, T> {}

impl<H: Hash, T: Hash> Hash for ThinArc<H, T> {
    #[inline]
    fn hash<S: Hasher>(&self, hasher: &mut S) {
        ThinArc::with_arc(self, |a| a.hash(hasher))
    }
}

#[cfg(test)]
mod tests {
    use crate::{Arc, HeaderWithLength, ThinArc};
    use alloc::vec;
    use core::clone::Clone;
    use core::ops::Drop;
    use core::sync::atomic;
    use core::sync::atomic::Ordering::{Acquire, SeqCst};

    #[derive(PartialEq)]
    struct Canary(*mut atomic::AtomicUsize);

    impl Drop for Canary {
        fn drop(&mut self) {
            unsafe {
                (*self.0).fetch_add(1, SeqCst);
            }
        }
    }

    #[test]
    fn empty_thin() {
        let header = HeaderWithLength::new(100u32, 0);
        let x = Arc::from_header_and_iter(header, core::iter::empty::<i32>());
        let y = Arc::into_thin(x.clone());
        assert_eq!(y.header.header, 100);
        assert!(y.slice.is_empty());
        assert_eq!(x.header.header, 100);
        assert!(x.slice.is_empty());
    }

    #[test]
    fn thin_assert_padding() {
        #[derive(Clone, Default)]
        #[repr(C)]
        struct Padded {
            i: u16,
        }

        // The header will have more alignment than `Padded`
        let header = HeaderWithLength::new(0i32, 2);
        let items = vec![Padded { i: 0xdead }, Padded { i: 0xbeef }];
        let a = ThinArc::from_header_and_iter(header, items.into_iter());
        assert_eq!(a.slice.len(), 2);
        assert_eq!(a.slice[0].i, 0xdead);
        assert_eq!(a.slice[1].i, 0xbeef);
    }

    #[test]
    fn slices_and_thin() {
        let mut canary = atomic::AtomicUsize::new(0);
        let c = Canary(&mut canary as *mut atomic::AtomicUsize);
        let v = vec![5, 6];
        let header = HeaderWithLength::new(c, v.len());
        {
            let x = Arc::into_thin(Arc::from_header_and_iter(header, v.into_iter()));
            let y = ThinArc::with_arc(&x, |q| q.clone());
            let _ = y.clone();
            let _ = x == x;
            Arc::from_thin(x.clone());
        }
        assert_eq!(canary.load(Acquire), 1);
    }
}
