//! Example demonstrating `SignalExt` methods for numeric types.

use nami::{Binding, Signal, SignalExt, binding};

fn main() {
    // negate: Get the negative of a number
    let temperature: Binding<i32> = binding(25);
    let inverted = temperature.negate();

    println!("Temperature: {}", temperature.snapshot()); // 25
    println!("Inverted: {}", inverted.snapshot()); // -25

    temperature.set(-10);
    println!("Temperature: {}", temperature.snapshot()); // -10
    println!("Inverted: {}", inverted.snapshot()); // 10

    // abs: Get absolute value
    let delta: Binding<i32> = binding(-42);
    let magnitude = delta.abs();

    println!("Delta: {}", delta.snapshot()); // -42
    println!("Magnitude: {}", magnitude.snapshot()); // 42

    delta.set(100);
    println!("Magnitude: {}", magnitude.snapshot()); // 100

    // sign: Check if value is non-negative (positive or zero)
    let balance: Binding<i32> = binding(100);
    let is_positive_or_zero = balance.sign();

    println!(
        "Balance {} is positive or zero: {}",
        balance.snapshot(),
        is_positive_or_zero.snapshot()
    ); // true

    balance.set(0);
    println!(
        "Balance {} is positive or zero: {}",
        balance.snapshot(),
        is_positive_or_zero.snapshot()
    ); // true

    balance.set(-50);
    println!(
        "Balance {} is positive or zero: {}",
        balance.snapshot(),
        is_positive_or_zero.snapshot()
    ); // false

    // is_positive, is_negative, is_zero
    let value: Binding<i32> = binding(42);

    let positive = value.is_positive();
    let negative = value.is_negative();
    let zero = value.is_zero();

    println!("{} is positive: {}", value.snapshot(), positive.snapshot()); // true
    println!("{} is negative: {}", value.snapshot(), negative.snapshot()); // false
    println!("{} is zero: {}", value.snapshot(), zero.snapshot()); // false

    value.set(-10);
    println!("{} is positive: {}", value.snapshot(), positive.snapshot()); // false
    println!("{} is negative: {}", value.snapshot(), negative.snapshot()); // true
    println!("{} is zero: {}", value.snapshot(), zero.snapshot()); // false

    value.set(0);
    println!("{} is positive: {}", value.snapshot(), positive.snapshot()); // false
    println!("{} is negative: {}", value.snapshot(), negative.snapshot()); // false
    println!("{} is zero: {}", value.snapshot(), zero.snapshot()); // true

    // Practical example: Show different UI based on balance
    let account_balance: Binding<i32> = binding(1500);
    // Store intermediate signal to avoid temporary lifetime issues
    let is_overdrawn = account_balance.is_negative();
    let status_color = is_overdrawn.select("red", "green");
    let status_text = is_overdrawn.select("Overdrawn!", "In good standing");

    println!("Balance: ${}", account_balance.snapshot());
    println!("Status color: {}", status_color.snapshot()); // green
    println!("Status: {}", status_text.snapshot()); // In good standing

    account_balance.set(-200);
    println!("\nBalance: ${}", account_balance.snapshot());
    println!("Status color: {}", status_color.snapshot()); // red
    println!("Status: {}", status_text.snapshot()); // Overdrawn!
}
