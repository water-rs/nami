use nami::binding;
use nami::collection::Collection;

fn takes<C: Collection>(_: C) {}

fn main() {
    let values = binding(vec![1, 2, 3]);
    takes(values);
}
