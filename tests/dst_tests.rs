use elysees::*;
use erasable::Thin;
use slice_dst::SliceWithHeader;
use std::borrow::BorrowMut;
use std::iter::FromIterator;

#[test]
fn basic_dst_test() {
    let vec = Vec::from_iter(0..100);
    let arc: Arc<_> = SliceWithHeader::new("header", 0..100);
    assert_eq!(
        std::mem::size_of_val(&arc),
        2 * std::mem::size_of::<usize>()
    );
    assert_eq!(arc.header, "header");
    assert_eq!(arc.slice, vec[..]);
    assert!(arc.is_unique());
    let thin: Thin<_> = arc.clone().into();
    assert_eq!(thin.header, "header");
    assert_eq!(thin.slice, vec[..]);
    assert_eq!(std::mem::size_of_val(&thin), std::mem::size_of::<usize>());
    let borrowed = arc.borrow_arc();
    assert_eq!(
        std::mem::size_of_val(&borrowed),
        2 * std::mem::size_of::<usize>()
    );
    let thin_borrowed: Thin<_> = borrowed.into();
    assert_eq!(
        std::mem::size_of_val(&thin_borrowed),
        std::mem::size_of::<usize>()
    );
}

#[test]
fn unique_dst_test() {
    let mut arc: ArcBox<_> = SliceWithHeader::new("unique", 0..5);
    assert_eq!(
        std::mem::size_of_val(&arc),
        2 * std::mem::size_of::<usize>()
    );
    assert_eq!(arc.header, "unique");
    assert_eq!(arc.slice, [0, 1, 2, 3, 4]);
    arc.slice[3] = 77;
    assert_eq!(arc.slice, [0, 1, 2, 77, 4]);
    arc.as_mut().slice[4] = 88;
    assert_eq!(arc.slice, [0, 1, 2, 77, 88]);
    let mut_borrow: &mut SliceWithHeader<_, _> = arc.borrow_mut();
    mut_borrow.slice[0] = 99;
    assert_eq!(arc.slice, [99, 1, 2, 77, 88]);

    let arc = arc.shareable();
    assert_eq!(arc.header, "unique");
    assert_eq!(arc.slice, [99, 1, 2, 77, 88]);
    
    let arc = Arc::try_unique(arc).expect("Is unique!");
    assert_eq!(arc.header, "unique");
    assert_eq!(arc.slice, [99, 1, 2, 77, 88]);

    let arc = arc.shareable();
    assert_eq!(arc.header, "unique");
    assert_eq!(arc.slice, [99, 1, 2, 77, 88]);
    let _arc2 = arc.clone();
    Arc::try_unique(arc).expect_err("Not unique!");
}
