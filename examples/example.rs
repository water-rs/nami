//! An example of how to use Nami.

use nami::{Binding, Signal, binding};

fn main() {
    // Demonstrates automatic type conversion with Into trait
    let text: Binding<String> = binding("hello world"); // &str -> String
    println!("Text value: {}", text.snapshot());

    // Direct initialization
    let counter: Binding<f64> = binding(42);
    println!("Counter: {}", counter.snapshot());

    // Update values - set() also accepts Into<T> for ergonomic usage
    text.set_from("updated text"); // No .into() needed!
    counter.add_assign(8.0);

    println!("\nAfter updates:");
    println!("Text: {}", text.snapshot());
    println!("Counter: {}", counter.snapshot());
}
