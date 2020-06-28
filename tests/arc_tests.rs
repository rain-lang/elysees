use elysees::*;
use lazy_static::lazy_static;
use std::borrow::{Borrow, BorrowMut};
use std::cmp::Ordering;
use std::collections::HashSet;
use std::ptr::NonNull;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Mutex;

#[derive(Debug, Eq, PartialEq, Hash)]
struct SyncPtr(*const ());

unsafe impl Send for SyncPtr {}
unsafe impl Sync for SyncPtr {}

lazy_static! {
    /// Set of roots for MIRI to treat as always reachable, to avoid memory leak errors
    static ref ROOTS: Mutex<HashSet<SyncPtr>> = Mutex::new(HashSet::new());
}

#[test]
fn basic_arc_usage() {
    let x = Arc::new(7);
    assert!(x.is_unique());
    assert_eq!(*x, 7);
    let y = x.clone();
    assert!(!x.is_unique());
    assert!(!y.is_unique());
    assert_eq!(Arc::count(&x, Relaxed), 2);
    assert_eq!(Arc::count(&y, Relaxed), 2);
    let x = Arc::try_unique(x).expect_err("x is not unique!");
    assert!(!x.is_unique());
    assert!(!y.is_unique());
    assert_eq!(x, y);
    assert_eq!(*x, 7);
    std::mem::drop(x);
    assert!(y.is_unique());
    let mut y = Arc::try_unique(y).expect("y is unique");
    *y += 73;
    assert_eq!(*y, 80);
    let y = y.shareable();
    assert!(y.is_unique());
    assert_eq!(*y, 80);

    let yb = y.borrow_arc();
    assert_eq!(*yb, 80);
    assert_eq!(ArcBorrow::count(yb, Relaxed), 1);
    let yb2 = yb;
    assert_eq!(ArcBorrow::count(yb, Relaxed), 1);
    assert_eq!(ArcBorrow::count(yb2, Relaxed), 1);
    let ybr = yb2.as_arc();
    assert_eq!(Arc::count(ybr, Relaxed), 1);
    assert!(ybr.is_unique());

    let z = y.clone();
    assert_eq!(*z, 80);
    let yl = Arc::leak(y);
    assert_eq!(ArcBorrow::count(yl, Relaxed), 2);
    assert_eq!(Arc::count(yl.as_arc(), Relaxed), 2);
    assert_eq!(Arc::count(&z, Relaxed), 2);
    let t = yl.as_arc().clone();
    assert_eq!(Arc::count(&t, Relaxed), 3);
    let w = yl.clone_arc();
    assert_eq!(Arc::count(&t, Relaxed), 4);
    assert_eq!(Arc::count(&z, Relaxed), 4);
    assert_eq!(Arc::count(&w, Relaxed), 4);
    assert_eq!(ArcBorrow::count(yl, Relaxed), 4);

    std::mem::drop(w);
    assert_eq!(Arc::count(&t, Relaxed), 3);
    assert_eq!(Arc::count(&z, Relaxed), 3);
    assert_eq!(ArcBorrow::count(yl, Relaxed), 3);

    std::mem::drop(t);
    assert_eq!(Arc::count(&z, Relaxed), 2);
    assert_eq!(ArcBorrow::count(yl, Relaxed), 2);

    std::mem::drop(z);
    assert_eq!(ArcBorrow::count(yl, Relaxed), 1);

    let mut make_unique = Arc::unique(yl.clone_arc());
    *make_unique += 23;
    assert_eq!(*make_unique, 103);
    *make_unique.as_mut() += 5;
    assert_eq!(*make_unique, 108);
    let borrowed_unique: &mut usize = make_unique.borrow_mut();
    *borrowed_unique += 5;
    assert_eq!(*borrowed_unique, 113);
    assert_eq!(*make_unique, 113);
    let mut make_unique = make_unique.shareable();
    assert_eq!(*make_unique, 113);
    assert!(make_unique.is_unique());
    assert_eq!(*yl, 80);
    let make_mut = Arc::make_mut(&mut make_unique);
    assert_eq!(*make_mut, 113);
    *make_mut += 100;

    assert_eq!(*make_mut, 213);
    assert_eq!(*make_unique, 213);
    assert!(&make_unique != yl.as_arc());
    assert!(&make_unique > yl.as_arc());
    assert!(&make_unique >= yl.as_arc());
    assert!(yl.as_arc() < &make_unique);
    assert!(yl.as_arc() <= &make_unique);
    assert_eq!(yl.as_arc().cmp(&make_unique), Ordering::Less);
    assert_eq!(yl.as_arc().partial_cmp(&make_unique), Some(Ordering::Less));

    let remake_unique = Arc::try_unique(make_unique).expect("Unique!");
    assert_eq!(*remake_unique, 213);
    let mut box_unique = remake_unique.clone();
    *box_unique += 100;
    assert_eq!(*box_unique, 313);
    assert_eq!(*remake_unique, 213);

    let box_unique = box_unique.shareable();
    let not_unique = box_unique.clone();
    let not_unique = Arc::try_unique(not_unique).expect_err("Not unique!");
    assert!(!box_unique.is_unique());

    let ptr_borrow: &*const usize = not_unique.borrow();
    let leak_ptr_borrow: &*const usize = yl.borrow();
    let unique_ptr_borrow: &*const usize = remake_unique.borrow();
    assert_eq!(*ptr_borrow, &*not_unique as *const _);
    assert_eq!(*leak_ptr_borrow, &*yl as *const _);
    assert_eq!(*unique_ptr_borrow, &*remake_unique as *const _);

    assert_eq!(*ptr_borrow, not_unique.borrow() as *const _);
    assert_eq!(*leak_ptr_borrow, yl.borrow() as *const _);
    assert_eq!(*unique_ptr_borrow, remake_unique.borrow() as *const _);
    assert_eq!(*ptr_borrow, not_unique.as_ref() as *const _);
    assert_eq!(*leak_ptr_borrow, yl.as_ref() as *const _);
    assert_eq!(*leak_ptr_borrow, yl.get() as *const _);
    assert_eq!(*unique_ptr_borrow, remake_unique.as_ref() as *const _);

    let ptr_borrow: &*const usize = not_unique.as_ref();
    let leak_ptr_borrow: &*const usize = yl.as_ref();
    let unique_ptr_borrow: &*const usize = remake_unique.as_ref();
    assert_eq!(*ptr_borrow, &*not_unique as *const _);
    assert_eq!(*leak_ptr_borrow, &*yl as *const _);
    assert_eq!(*unique_ptr_borrow, &*remake_unique as *const _);

    let ptr_borrow: &*mut usize = not_unique.borrow();
    let leak_ptr_borrow: &*mut usize = yl.borrow();
    let unique_ptr_borrow: &*mut usize = remake_unique.borrow();
    assert_eq!(*ptr_borrow as *const _, &*not_unique as *const _);
    assert_eq!(*leak_ptr_borrow as *const _, &*yl as *const _);
    assert_eq!(*unique_ptr_borrow as *const _, &*remake_unique as *const _);

    let ptr_borrow: &*mut usize = not_unique.as_ref();
    let leak_ptr_borrow: &*mut usize = yl.as_ref();
    let unique_ptr_borrow: &*mut usize = remake_unique.as_ref();
    assert_eq!(*ptr_borrow as *const _, &*not_unique as *const _);
    assert_eq!(*leak_ptr_borrow as *const _, &*yl as *const _);
    assert_eq!(*unique_ptr_borrow as *const _, &*remake_unique as *const _);

    let ptr_borrow: &NonNull<_> = not_unique.borrow();
    let leak_ptr_borrow: &NonNull<_> = yl.borrow();
    let unique_ptr_borrow: &NonNull<_> = remake_unique.borrow();
    assert_eq!(ptr_borrow.as_ptr() as *const _, &*not_unique as *const _);
    assert_eq!(leak_ptr_borrow.as_ptr() as *const _, &*yl as *const _);
    assert_eq!(
        unique_ptr_borrow.as_ptr() as *const _,
        &*remake_unique as *const _
    );

    let ptr_borrow: &NonNull<_> = not_unique.as_ref();
    let leak_ptr_borrow: &NonNull<_> = yl.as_ref();
    let unique_ptr_borrow: &NonNull<_> = remake_unique.as_ref();
    assert_eq!(ptr_borrow.as_ptr() as *const _, &*not_unique as *const _);
    assert_eq!(leak_ptr_borrow.as_ptr() as *const _, &*yl as *const _);
    assert_eq!(
        unique_ptr_borrow.as_ptr() as *const _,
        &*remake_unique as *const _
    );

    let leak_ref_borrow: &&_ = yl.borrow();
    assert_eq!(*leak_ref_borrow, yl.get());
    let leak_ref_borrow: &&_ = yl.as_ref();
    assert_eq!(*leak_ref_borrow, yl.get());

    let yba: &Arc<_> = yl.borrow();
    let yaa: &Arc<_> = yl.as_ref();
    assert_eq!(yba, yaa);
    assert!(ArcBorrow::ptr_eq(yba.borrow_arc(), yaa.borrow_arc()));
    assert!(ArcBorrow::ptr_eq(yba.borrow_arc(), yl));

    // Avoid memory leaK error for yl
    ROOTS
        .lock()
        .unwrap()
        .insert(SyncPtr(ArcBorrow::into_raw(yl) as *const ()));
}

#[test]
fn arc_formatting() {
    let arc1 = Arc::new(56);
    let arc2 = Arc::new(88);
    assert_eq!(format!("{:?}", arc1), "56");
    assert_eq!(format!("{:?}", arc2), "88");
    assert_eq!(format!("{}", arc1), "56");
    assert_eq!(format!("{}", arc2), "88");
    assert_eq!(format!("{:p}", arc1), format!("{:?}", &*arc1 as *const _));
    assert_eq!(format!("{:p}", arc2), format!("{:?}", &*arc2 as *const _));
}

#[test]
fn arc_default() {
    let arc: Arc<usize> = Arc::default();
    assert_eq!(*arc, 0);
    assert!(arc.is_unique());
    let unique_arc: ArcBox<usize> = ArcBox::default();
    assert_eq!(*unique_arc, 0);
}

#[test]
fn arc_hash() {
    let mut map = HashSet::new();
    assert!(map.insert(Arc::new(7)));
    assert!(map.insert(Arc::new(8)));
    assert!(map.insert(Arc::new(9)));
    assert!(!map.insert(Arc::new(7)));
}
