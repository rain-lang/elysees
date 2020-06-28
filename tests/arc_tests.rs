use elysees::*;
use std::ops::Deref;
use std::sync::atomic::Ordering;

#[test]
fn basic_arc_creation_works() {
    let x = Arc::new(7);
    assert!(x.is_unique());
    assert_eq!(*x.deref(), 7);
    let y = x.clone();
    assert!(!x.is_unique());
    assert!(!y.is_unique());
    assert_eq!(Arc::count(&x, Ordering::Relaxed), 2);
    assert_eq!(Arc::count(&y, Ordering::Relaxed), 2);
    let x = Arc::try_unique(x).expect_err("x is not unique!");
    assert!(!x.is_unique());
    assert!(!y.is_unique());
    assert_eq!(x, y);
    assert_eq!(*x.deref(), 7);
    std::mem::drop(x);
    assert!(y.is_unique());
    let mut y = Arc::try_unique(y).expect("y is unique");
    *y += 73;
    assert_eq!(*y.deref(), 80);
    let y = y.shareable();
    assert!(y.is_unique());
    assert_eq!(*y.deref(), 80);

    let yb = y.borrow_arc();
    assert_eq!(*yb.deref(), 80);
    assert_eq!(ArcBorrow::count(yb, Ordering::Relaxed), 1);
    let yb2 = yb.clone();
    assert_eq!(ArcBorrow::count(yb, Ordering::Relaxed), 1);
    assert_eq!(ArcBorrow::count(yb2, Ordering::Relaxed), 1);
    let ybr = yb2.as_arc();
    assert_eq!(Arc::count(ybr, Ordering::Relaxed), 1);
    assert!(ybr.is_unique());

    let z = y.clone();
    assert_eq!(*z.deref(), 80);
    let yl = Arc::leak(y);
    assert_eq!(ArcBorrow::count(yl, Ordering::Relaxed), 2);
    assert_eq!(Arc::count(yl.as_arc(), Ordering::Relaxed), 2);
    assert_eq!(Arc::count(&z, Ordering::Relaxed), 2);
    let t = yl.as_arc().clone();
    assert_eq!(Arc::count(&t, Ordering::Relaxed), 3);
    let w = yl.clone_arc();
    assert_eq!(Arc::count(&t, Ordering::Relaxed), 4);
    assert_eq!(Arc::count(&z, Ordering::Relaxed), 4);
    assert_eq!(Arc::count(&w, Ordering::Relaxed), 4);
    assert_eq!(ArcBorrow::count(yl, Ordering::Relaxed), 4);

    std::mem::drop(w);
    assert_eq!(Arc::count(&t, Ordering::Relaxed), 3);
    assert_eq!(Arc::count(&z, Ordering::Relaxed), 3);
    assert_eq!(ArcBorrow::count(yl, Ordering::Relaxed), 3);

    std::mem::drop(t);
    assert_eq!(Arc::count(&z, Ordering::Relaxed), 2);
    assert_eq!(ArcBorrow::count(yl, Ordering::Relaxed), 2);

    std::mem::drop(z);
    assert_eq!(ArcBorrow::count(yl, Ordering::Relaxed), 1);
}
