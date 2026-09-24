//! Example demonstrating `SignalExt` comparison methods.

use nami::{Binding, Signal, SignalExt, binding};

fn main() {
    // equal_to: Check if a value equals a specific value
    let score: Binding<i32> = binding(100);
    let is_perfect = score.equal_to(100);
    println!("Is score perfect? {}", is_perfect.snapshot()); // true

    score.set(95);
    println!("Is score perfect? {}", is_perfect.snapshot()); // false

    // For not-equal checks, use equal_to().not()
    let is_not_perfect = score.equal_to(100).not();
    println!("Is score not perfect? {}", is_not_perfect.snapshot()); // true

    // condition: Custom predicate
    let number: Binding<i32> = binding(42);
    let is_even = number.condition(|n| n % 2 == 0);
    let is_divisible_by_7 = number.condition(|n| n % 7 == 0);

    println!("Is {} even? {}", number.snapshot(), is_even.snapshot()); // true
    println!(
        "Is {} divisible by 7? {}",
        number.snapshot(),
        is_divisible_by_7.snapshot()
    ); // true

    number.set(15);
    println!("Is {} even? {}", number.snapshot(), is_even.snapshot()); // false
    println!(
        "Is {} divisible by 7? {}",
        number.snapshot(),
        is_divisible_by_7.snapshot()
    ); // false
}
