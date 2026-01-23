//! Example demonstrating SignalExt methods for Result types.

use nami::{Binding, Signal, SignalExt, binding};

fn main() {
    // Basic Result checks
    let api_response: Binding<Result<String, String>> = binding(Ok("Success!".to_string()));

    let is_success = api_response.is_ok();
    let is_error = api_response.is_err();

    println!("Is success: {}", is_success.get()); // true
    println!("Is error: {}", is_error.get()); // false

    api_response.set(Err("Network error".to_string()));
    println!("Is success: {}", is_success.get()); // false
    println!("Is error: {}", is_error.get()); // true

    // ok: Convert Result to Option<T>
    let result: Binding<Result<i32, &str>> = binding(Ok(42));
    let maybe_value = result.ok();

    println!("Value: {:?}", maybe_value.get()); // Some(42)

    result.set(Err("error"));
    println!("Value: {:?}", maybe_value.get()); // None

    // err: Convert Result to Option<E>
    let result2: Binding<Result<i32, &str>> = binding(Err("something went wrong"));
    let maybe_error = result2.err();

    println!("Error: {:?}", maybe_error.get()); // Some("something went wrong")

    result2.set(Ok(100));
    println!("Error: {:?}", maybe_error.get()); // None

    // Practical example: API response handling
    #[derive(Clone, Debug)]
    #[allow(unused)]
    struct User {
        name: String,
        email: String,
    }

    #[derive(Clone, Debug)]
    #[allow(unused)]
    struct ApiError {
        code: i32,
        message: String,
    }

    let user_fetch: Binding<Result<User, ApiError>> = binding(Ok(User {
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
    }));

    // Derive signals for different UI states
    let show_content = user_fetch.is_ok();
    let show_error = user_fetch.is_err();

    println!("\n--- User loaded ---");
    println!("Show content: {}", show_content.get()); // true
    println!("Show error: {}", show_error.get()); // false

    // Simulate an error
    user_fetch.set(Err(ApiError {
        code: 404,
        message: "User not found".to_string(),
    }));

    println!("\n--- Error occurred ---");
    println!("Show content: {}", show_content.get()); // false
    println!("Show error: {}", show_error.get()); // true

    // Chain with map to extract specific fields
    let user_result: Binding<Result<User, ApiError>> = binding(Ok(User {
        name: "Bob".to_string(),
        email: "bob@example.com".to_string(),
    }));

    // Get the username if successful, or "Unknown" if error
    let display_name = user_result
        .ok()
        .map(|opt| opt.map(|u| u.name).unwrap_or_else(|| "Unknown".to_string()));

    println!("\nDisplay name: {}", display_name.get()); // Bob

    user_result.set(Err(ApiError {
        code: 500,
        message: "Server error".to_string(),
    }));
    println!("Display name: {}", display_name.get()); // Unknown
}
