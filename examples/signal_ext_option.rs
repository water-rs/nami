//! Example demonstrating `SignalExt` methods for Option types.

use nami::{Binding, Signal, SignalExt, binding};

fn main() {
    // Basic Option checks
    let maybe_user: Binding<Option<String>> = binding(Some("Alice".to_string()));

    let has_user = maybe_user.is_some();
    let no_user = maybe_user.is_none();

    println!("Has user: {}", has_user.snapshot()); // true
    println!("No user: {}", no_user.snapshot()); // false

    // unwrap_or: Provide a default value
    let username = maybe_user.unwrap_or("Guest".to_string());
    println!("Username: {}", username.snapshot()); // Alice

    maybe_user.set(None);
    println!("Username: {}", username.snapshot()); // Guest
    println!("Has user: {}", has_user.snapshot()); // false

    // unwrap_or_else: Compute default lazily
    let count: Binding<Option<i32>> = binding(Some(42));
    let value = count.unwrap_or_else(|| {
        println!("Computing default..."); // Only called when None
        0
    });
    println!("Value: {}", value.snapshot()); // 42

    count.set(None);
    println!("Value: {}", value.snapshot()); // prints "Computing default..." then 0

    // unwrap_or_default: Use Default trait
    let optional_vec: Binding<Option<Vec<i32>>> = binding(None);
    let vec_value = optional_vec.unwrap_or_default();
    println!("Vec length: {}", vec_value.snapshot().len()); // 0

    // some_equal_to: Check if Some contains a specific value
    let selection: Binding<Option<i32>> = binding(Some(1));
    let is_option_1 = selection.some_equal_to(1);
    let is_option_2 = selection.some_equal_to(2);

    println!("Selected option 1? {}", is_option_1.snapshot()); // true
    println!("Selected option 2? {}", is_option_2.snapshot()); // false

    selection.set(Some(2));
    println!("Selected option 1? {}", is_option_1.snapshot()); // false
    println!("Selected option 2? {}", is_option_2.snapshot()); // true

    selection.set(None);
    println!("Selected option 1? {}", is_option_1.snapshot()); // false (None doesn't match)

    // flatten: Collapse nested Options
    let nested: Binding<Option<Option<i32>>> = binding(Some(Some(42)));
    let flattened = nested.flatten();
    println!("Flattened: {:?}", flattened.snapshot()); // Some(42)

    nested.set(Some(None));
    println!("Flattened: {:?}", flattened.snapshot()); // None

    nested.set(None);
    println!("Flattened: {:?}", flattened.snapshot()); // None
}
