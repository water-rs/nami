//! Example demonstrating SignalExt methods for bool types.

use nami::{Binding, Signal, SignalExt, binding};

fn main() {
    // not: Logical negation
    let is_logged_in: Binding<bool> = binding(true);
    let is_logged_out = is_logged_in.not();

    println!("Logged in: {}", is_logged_in.get()); // true
    println!("Logged out: {}", is_logged_out.get()); // false

    is_logged_in.set(false);
    println!("Logged in: {}", is_logged_in.get()); // false
    println!("Logged out: {}", is_logged_out.get()); // true

    // then_some: Convert bool to Option
    let show_message: Binding<bool> = binding(true);
    let message = show_message.then_some("Welcome back!");

    println!("Message: {:?}", message.get()); // Some("Welcome back!")

    show_message.set(false);
    println!("Message: {:?}", message.get()); // None

    // select: Choose between two values based on condition
    let dark_mode: Binding<bool> = binding(true);
    let theme = dark_mode.select("dark-theme", "light-theme");
    let background = dark_mode.select("#1a1a2e", "#ffffff");

    println!("Theme: {}", theme.get()); // dark-theme
    println!("Background: {}", background.get()); // #1a1a2e

    dark_mode.set(false);
    println!("Theme: {}", theme.get()); // light-theme
    println!("Background: {}", background.get()); // #ffffff

    // Combining bool methods
    let is_admin: Binding<bool> = binding(false);
    let has_permission: Binding<bool> = binding(true);

    // Use zip to combine conditions
    let can_edit = is_admin
        .zip(&has_permission)
        .map(|(admin, perm)| admin || perm);
    println!("Can edit: {}", can_edit.get()); // true (has permission)

    has_permission.set(false);
    println!("Can edit: {}", can_edit.get()); // false

    is_admin.set(true);
    println!("Can edit: {}", can_edit.get()); // true (is admin)
}
