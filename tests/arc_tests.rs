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
}