//! An example of how to use Nami.

use nami::{Signal, binding};

fn main() {
    let text = binding(String::from("hello world"));
    println!("Text value: {}", text.snapshot());

    let counter = binding(42.0_f64);
    println!("Counter: {}", counter.snapshot());

    // Update values - set_from() accepts Into<T> for ergonomic usage
    text.set_from("updated text"); // No .into() needed!
    counter.add_assign(8.0);

    println!("\nAfter updates:");
    println!("Text: {}", text.snapshot());
    println!("Counter: {}", counter.snapshot());
}
