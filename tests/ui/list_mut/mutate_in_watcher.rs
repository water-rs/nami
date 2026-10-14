//! `List` mutators take `&mut self`, so an `Fn` watcher cannot mutate a list
//! it captured.

use nami::collection::{Collection, List};
use nami::SelfId;

fn main() {
    let list: List<SelfId<i32>> = List::new();
    let captured = list.clone();
    let _guard = list.watch(.., move |_items, _change| {
        captured.push(SelfId::new(1));
    });
}
