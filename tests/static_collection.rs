//! Immutable static sequences are collections identified by index.

use nami::collection::{AnyCollection, Collection, CollectionChange};

#[test]
fn str_array_is_a_collection_with_index_ids() {
    fn assert_index_ids<C: Collection<Id = usize>>(_: &C) {}

    let names: [&str; 3] = ["a", "b", "c"];
    assert_index_ids(&names);
    assert_eq!(Collection::len(&names), 3);
    assert_eq!(Collection::get(&names, 1), Some("b"));
    assert_eq!(Collection::id(&names, 0), Some(0));
    assert_eq!(Collection::id(&names, 2), Some(2));
    assert_eq!(Collection::id(&names, 3), None);

    // Static collections never notify.
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let calls2 = calls.clone();
    names.watch(.., move |_items, _change: &CollectionChange| {
        calls2.set(calls2.get() + 1);
    });
    assert_eq!(calls.get(), 0);
}

#[test]
fn static_slices_box_and_rc_use_index_ids() {
    let slice: &'static [&str] = &["x", "y"];
    let rc: std::rc::Rc<[&str]> = std::rc::Rc::from(["x", "y"]);
    let boxed: Box<[&str]> = Box::from(["x", "y"]);
    assert_eq!(Collection::id(&slice, 1), Some(1));
    assert_eq!(Collection::id(&rc, 1), Some(1));
    assert_eq!(Collection::id(&boxed, 1), Some(1));

    let any: AnyCollection<&str, usize> = AnyCollection::new(["p", "q"]);
    assert_eq!(any.id(1), Some(1));
    assert_eq!(any.get(0), Some("p"));
}

#[test]
fn vec_still_uses_item_ids() {
    let items = vec![nami::SelfId::new(10_u32), nami::SelfId::new(20)];
    assert_eq!(Collection::id(&items, 1), Some(20));
}
