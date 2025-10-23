#![allow(missing_docs)]

use nami::*;

#[test]
fn test_s_macro_constant() {
    let s = s!("Hello, world!");
    assert_eq!(s.get(), "Hello, world!");
}

#[test]
fn test_s_macro_positional_args() {
    let name = constant("Alice");
    let age = constant(30);

    // 1 argument
    let s1 = s!("Hello, {}!", name);
    assert_eq!(s1.get(), "Hello, Alice!");

    // 2 arguments
    let s2 = s!("{} is {} years old.", name, age);
    assert_eq!(s2.get(), "Alice is 30 years old.");

    let s3 = s!(
        "{} {} {} {}",
        constant(1),
        constant(2),
        constant(3),
        constant(4)
    );
    assert_eq!(s3.get(), "1 2 3 4");
}

#[test]
fn test_s_macro_named_args() {
    let name = constant("Bob");
    let age = constant(42);

    // 1 argument
    let s1 = s!("Hello, {name}!");
    assert_eq!(s1.get(), "Hello, Bob!");

    // 2 arguments
    let s2 = s!("{name} is {age} years old.");
    assert_eq!(s2.get(), "Bob is 42 years old.");

    let a = constant(1);
    let b = constant(2);
    let c = constant(3);
    let d = constant(4);
    let s3 = s!("{a} {b} {c} {d}");
    assert_eq!(s3.get(), "1 2 3 4");
}

#[test]
fn test_s_macro_reactivity_positional() {
    let mut name = binding("Alice".to_string());
    let s = s!("Hello, {}!", name.clone());

    assert_eq!(s.get(), "Hello, Alice!");

    name.set("Bob".to_string());
    assert_eq!(s.get(), "Hello, Bob!");
}

#[test]
fn test_s_macro_reactivity_named() {
    let mut name = binding("Alice".to_string());
    let s = s!("Hello, {name}!");

    assert_eq!(s.get(), "Hello, Alice!");

    name.set("Bob".to_string());
    assert_eq!(s.get(), "Hello, Bob!");
}

#[test]
fn test_s_macro_escaped_braces() {
    let s = s!("This should have {{escaped}} braces.");
    assert_eq!(s.get(), "This should have {escaped} braces.");
}
