use nami::signal::IntoComputed;
use nami::{Binding, binding};

fn takes<T: IntoComputed<f32>>(_: T) {}

fn main() {
    let value: Binding<f32> = binding(1.0f32);
    takes(&value);
}
