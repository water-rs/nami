//! Example demonstrating `SignalExt` methods for numeric types.

use nami::{Binding, Signal, SignalExt, binding};

fn main() {
    // negate: Get the negative of a number
    let temperature: Binding<i32> = binding(25);
    let inverted = temperature.negate();

    println!("Temperature: {}", temperature.get()); // 25
    println!("Inverted: {}", inverted.get()); // -25

    temperature.set(-10);
    println!("Temperature: {}", temperature.get()); // -10
    println!("Inverted: {}", inverted.get()); // 10

    // abs: Get absolute value
    let delta: Binding<i32> = binding(-42);
    let magnitude = delta.abs();

    println!("Delta: {}", delta.get()); // -42
    println!("Magnitude: {}", magnitude.get()); // 42

    delta.set(100);
    println!("Magnitude: {}", magnitude.get()); // 100

    // sign: Check if value is non-negative (positive or zero)
    let balance: Binding<i32> = binding(100);
    let is_positive_or_zero = balance.sign();

    println!(
        "Balance {} is positive or zero: {}",
        balance.get(),
        is_positive_or_zero.get()
    ); // true

    balance.set(0);
    println!(
        "Balance {} is positive or zero: {}",
        balance.get(),
        is_positive_or_zero.get()
    ); // true

    balance.set(-50);
    println!(
        "Balance {} is positive or zero: {}",
        balance.get(),
        is_positive_or_zero.get()
    ); // false

    // is_positive, is_negative, is_zero
    let value: Binding<i32> = binding(42);

    let positive = value.is_positive();
    let negative = value.is_negative();
    let zero = value.is_zero();

    println!("{} is positive: {}", value.get(), positive.get()); // true
    println!("{} is negative: {}", value.get(), negative.get()); // false
    println!("{} is zero: {}", value.get(), zero.get()); // false

    value.set(-10);
    println!("{} is positive: {}", value.get(), positive.get()); // false
    println!("{} is negative: {}", value.get(), negative.get()); // true
    println!("{} is zero: {}", value.get(), zero.get()); // false

    value.set(0);
    println!("{} is positive: {}", value.get(), positive.get()); // false
    println!("{} is negative: {}", value.get(), negative.get()); // false
    println!("{} is zero: {}", value.get(), zero.get()); // true

    // Practical example: Show different UI based on balance
    let account_balance: Binding<i32> = binding(1500);
    // Store intermediate signal to avoid temporary lifetime issues
    let is_overdrawn = account_balance.is_negative();
    let status_color = is_overdrawn.select("red", "green");
    let status_text = is_overdrawn.select("Overdrawn!", "In good standing");

    println!("Balance: ${}", account_balance.get());
    println!("Status color: {}", status_color.get()); // green
    println!("Status: {}", status_text.get()); // In good standing

    account_balance.set(-200);
    println!("\nBalance: ${}", account_balance.get());
    println!("Status color: {}", status_color.get()); // red
    println!("Status: {}", status_text.get()); // Overdrawn!
}
