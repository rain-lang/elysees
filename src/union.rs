use crate::{Arc, ArcBorrow, ArcBox};
use erasable::ErasablePtr;
use erasable::Thin;
use ptr_union::{Builder2, Builder4, Union2, Union4};

/// A value which can be made into *any* pointer union
pub unsafe trait UnionAlign: Sized + ErasablePtr {
    fn left<B: UnionAlign>(this: Self) -> Union2<Self, B> {
        unsafe { Builder2::new_unchecked().a(this) }
    }
    fn right<A: UnionAlign>(this: Self) -> Union2<A, Self> {
        unsafe { Builder2::new_unchecked().b(this) }
    }
    fn a<B: UnionAlign, C: UnionAlign, D: UnionAlign>(this: Self) -> Union4<Self, B, C, D> {
        unsafe { Builder4::new_unchecked().a(this) }
    }
    fn b<A: UnionAlign, C: UnionAlign, D: UnionAlign>(this: Self) -> Union4<A, Self, C, D> {
        unsafe { Builder4::new_unchecked().b(this) }
    }
    fn c<A: UnionAlign, B: UnionAlign, D: UnionAlign>(this: Self) -> Union4<A, B, Self, D> {
        unsafe { Builder4::new_unchecked().c(this) }
    }
    fn d<A: UnionAlign, B: UnionAlign, C: UnionAlign>(this: Self) -> Union4<A, B, C, Self> {
        unsafe { Builder4::new_unchecked().d(this) }
    }
}

unsafe impl<T: ?Sized> UnionAlign for Arc<T> where Arc<T>: ErasablePtr {}
unsafe impl<T: ?Sized> UnionAlign for Thin<Arc<T>>
where
    Thin<Arc<T>>: ErasablePtr,
    Arc<T>: ErasablePtr,
{
}
unsafe impl<'a, T: ?Sized> UnionAlign for ArcBorrow<'a, T> where ArcBorrow<'a, T>: ErasablePtr {}
unsafe impl<T: ?Sized> UnionAlign for ArcBox<T> where ArcBox<T>: ErasablePtr {}
