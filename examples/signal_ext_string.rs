//! Example demonstrating SignalExt methods for string types.

use nami::{binding, Binding, Signal, SignalExt};

fn main() {
    // is_empty: Check if string is empty
    let username: Binding<String> = binding(String::new());
    let is_empty = username.is_empty();
    let is_valid = username.is_empty().not();

    println!("Username empty: {}", is_empty.get()); // true
    println!("Username valid: {}", is_valid.get()); // false

    username.set("alice".to_string());
    println!("Username empty: {}", is_empty.get()); // false
    println!("Username valid: {}", is_valid.get()); // true

    // str_len: Get string length
    let message: Binding<String> = binding("Hello, World!".to_string());
    let char_count = message.str_len();

    println!("Message length: {}", char_count.get()); // 13

    message.set("Hi".to_string());
    println!("Message length: {}", char_count.get()); // 2

    // contains: Check if string contains a pattern
    let search_query: Binding<String> = binding("rust programming language".to_string());
    let has_rust = search_query.contains("rust");
    let has_python = search_query.contains("python");

    println!("Contains 'rust': {}", has_rust.get()); // true
    println!("Contains 'python': {}", has_python.get()); // false

    search_query.set("python scripting".to_string());
    println!("Contains 'rust': {}", has_rust.get()); // false
    println!("Contains 'python': {}", has_python.get()); // true

    // Works with &str too
    let status: Binding<&str> = binding("loading...");
    let is_loading = status.contains("loading");
    let is_done = status.is_empty().not();

    println!("Is loading: {}", is_loading.get()); // true
    println!("Has status: {}", is_done.get()); // true

    status.set("complete");
    println!("Is loading: {}", is_loading.get()); // false

    // Combining string methods with other SignalExt methods
    let input: Binding<String> = binding(String::new());

    // Validation: non-empty and at least 3 characters
    let is_not_empty = input.is_empty().not();
    let is_long_enough = input.str_len().ge(3);
    let is_valid_input = is_not_empty.zip(&is_long_enough).map(|(a, b)| a && b);

    println!("Valid input: {}", is_valid_input.get()); // false (empty)

    input.set("ab".to_string());
    println!("Valid input: {}", is_valid_input.get()); // false (too short)

    input.set("abc".to_string());
    println!("Valid input: {}", is_valid_input.get()); // true
}
