use elysees::*;
use slice_dst::SliceWithHeader;
use erasable::Thin;
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
    let thin: Thin<_> = arc.into();
    assert_eq!(thin.header, "header");
    assert_eq!(thin.slice, vec[..]);
    assert_eq!(
        std::mem::size_of_val(&thin),
        std::mem::size_of::<usize>()
    )
}
