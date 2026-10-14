//! `s!` used from a crate whose only path to nami is a renamed dependency
//! (water-rs/nami#32). This crate compiling is most of the test.

use reactive::{Computed, SignalExt, constant, s};

/// Every `s!` mode, expanded where `::nami` does not resolve.
#[must_use]
pub fn greetings() -> [Computed<String>; 4] {
    let name = constant("Alice");
    let age = constant(30);
    [
        s!("Hello").computed(),
        s!("Hello {name}, {age}").computed(),
        s!("Hello {}", name).computed(),
        s!("Hello {n}", n = &name).computed(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use reactive::Signal;

    #[test]
    fn s_expands_through_the_renamed_crate() {
        let values = greetings().map(|greeting| greeting.snapshot());
        assert_eq!(
            values,
            ["Hello", "Hello Alice, 30", "Hello Alice", "Hello Alice"].map(String::from)
        );
    }
}
