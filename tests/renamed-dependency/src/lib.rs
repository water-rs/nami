//! `s!` and `derive(Project)` used from a crate whose only path to nami is a
//! renamed dependency (water-rs/nami#32, water-rs/nami#41). This crate
//! compiling is most of the test.

use reactive::{Computed, Project, SignalExt, constant, s};

/// A projectable struct whose expansion names nami as `reactive`.
#[derive(Clone, Debug, Project)]
#[project(crate = reactive)]
pub struct Person {
    /// The person's name.
    pub name: String,
    /// The person's age.
    pub age: u32,
}

/// A projectable tuple struct through the renamed crate.
#[derive(Clone, Debug, Project)]
#[project(crate = reactive)]
pub struct Pair(pub u32, pub u32);

/// A projectable unit struct through the renamed crate.
#[derive(Clone, Debug, Project)]
#[project(crate = reactive)]
pub struct Unit;

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
    use reactive::{Binding, Signal, binding};

    #[test]
    fn s_expands_through_the_renamed_crate() {
        let values = greetings().map(|greeting| greeting.snapshot());
        assert_eq!(
            values,
            ["Hello", "Hello Alice, 30", "Hello Alice", "Hello Alice"].map(String::from)
        );
    }

    #[test]
    fn project_expands_through_the_renamed_crate() {
        let person: Binding<Person> = binding(Person {
            name: "Alice".to_owned(),
            age: 30,
        });
        let projected = person.project();
        projected.age.set(31);
        assert_eq!(person.snapshot().age, 31);

        let pair: Binding<Pair> = binding(Pair(1, 2));
        let (_, second) = pair.project();
        second.set(3);
        assert_eq!(pair.snapshot().1, 3);

        let unit: Binding<Unit> = binding(Unit);
        let () = unit.project();
    }
}
