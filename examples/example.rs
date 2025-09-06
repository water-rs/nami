use nami::{Binding, binding};

fn main() {
    // Demonstrates automatic type conversion with Into trait
    let text: Binding<String> = binding("hello world");  // &str -> String
    println!("Text value: {}", text.get());
    
    // Direct initialization
    let counter = binding(42i32);
    println!("Counter: {}", counter.get());
    
    // Works with collections
    let items = binding(vec![1, 2, 3]);
    println!("Items: {:?}", items.get());
    
    // Update values - set() also accepts Into<T> for ergonomic usage
    text.set("updated text");  // No .into() needed!
    counter.increment(8);
    items.push(4);
    
    println!("\nAfter updates:");
    println!("Text: {}", text.get());
    println!("Counter: {}", counter.get());
    println!("Items: {:?}", items.get());
}
