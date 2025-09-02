use nami::{Binding, binding, s};

fn main() {
    let binding: Binding<String> = binding("value");
    s!("s {binding}");
}
