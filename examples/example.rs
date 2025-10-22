//! An example of how to use Nami.

use nami::{Binding, binding};

fn main() {
    // Demonstrates automatic type conversion with Into trait
    let mut text: Binding<String> = binding("hello world"); // &str -> String
    println!("Text value: {}", text.get());

    // Direct initialization
    let mut counter: Binding<f64> = binding(42);
    println!("Counter: {}", counter.get());

    // Update values - set() also accepts Into<T> for ergonomic usage
    text.set_from("updated text"); // No .into() needed!
    counter += 8.0;

    println!("\nAfter updates:");
    println!("Text: {}", text.get());
    println!("Counter: {}", counter.get());
}
