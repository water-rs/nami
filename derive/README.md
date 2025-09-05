# nami-derive

Derive and helper macros for the [nami](https://github.com/water-rs/nami) reactive framework.

This crate provides procedural macros to make reactive code more ergonomic, including
field projection for structs and an `s!` macro that builds formatted string signals.

## Features

- **`#[derive(Project)]`** - Automatically implement the `Project` trait for structs, enabling decomposition into individual field bindings

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
nami = { version = "0.3", features = ["derive"] }
```

The macros are re-exported through the main `nami` crate when the `derive` feature is enabled (default).

## `#[derive(Project)]`

The `Project` derive macro automatically generates implementations that allow you to decompose struct bindings into separate bindings for each field.

### Named Structs

For structs with named fields, the macro generates a corresponding "projected" struct where each field is wrapped in a `Binding`:

```rust
use nami::{Binding, binding};

#[derive(nami::Project)]
struct Person {
    name: String,
    age: u32,
    email: String,
}

let person: Binding<Person> = binding(Person {
    name: "Alice".to_string(),
    age: 30,
    email: "alice@example.com".to_string(),
});

// Project into individual field bindings
let projected = person.project();

// Access and modify individual fields
projected.name.set("Bob".to_string());
projected.age.set(25);

// Changes are reflected in the original binding
let updated_person = person.get();
assert_eq!(updated_person.name, "Bob");
assert_eq!(updated_person.age, 25);
assert_eq!(updated_person.email, "alice@example.com"); // unchanged
```

### Tuple Structs

For tuple structs, the macro generates a tuple of bindings:

```rust
use nami::{Binding, binding};

#[derive(nami::Project)]
struct Point(i32, i32);

let point: Binding<Point> = binding(Point(10, 20));
let (x, y) = point.project();

x.set(100);
y.set(200);

assert_eq!(point.get().0, 100);
assert_eq!(point.get().1, 200);
```

### Unit Structs

Unit structs project to the unit type `()`:

```rust
use nami::{Binding, binding};

#[derive(nami::Project)]
struct Marker;

let marker: Binding<Marker> = binding(Marker);
let _unit = marker.project(); // Returns ()
```

### Generic Types

The derive macro supports generic types with appropriate lifetime bounds:

```rust
use nami::{Binding, binding};

#[derive(nami::Project)]
struct Container<T> {
    value: T,
    count: usize,
}

let container: Binding<Container<String>> = binding(Container {
    value: "hello",
    count: 5,
});

let projected = container.project();
projected.value.set("world");
projected.count.set(10);
```

## Bidirectional Reactivity

All projected bindings maintain bidirectional reactivity with the original binding:

- Changes to projected bindings update the corresponding field in the original
- Changes to the original binding are reflected in the projected bindings
- The reactive system ensures efficient updates and notifications

## Limitations

- The derive macro only supports structs (not enums or unions)
- All field types must implement `Clone` and have `'static` lifetime
- Generic parameters automatically get `'static` bounds added

## License

This project is licensed under the MIT License - see the [LICENSE](../LICENSE) file for details.

## `s!` macro

Creates a formatted string signal, automatically capturing named variables from the format string.

```rust
use nami::*;

let name = constant("Alice");
let age = constant(25);

// Automatic variable capture from format string
let msg = s!("Hello {name}, you are {age} years old");

// Positional arguments still work
let msg2 = s!("Hello {}, you are {}", name, age);
```
