//! `StateList` mutators take `&mut self`, so an `Fn` watcher cannot mutate a list
//! it captured.

use nami::collection::{Collection, StateList};
use nami::SelfId;

fn main() {
    let list: StateList<SelfId<i32>> = StateList::new();
    let captured = list.clone();
    let _guard = list.watch(.., move |_items, _change| {
        captured.push(SelfId::new(1));
    });
}
