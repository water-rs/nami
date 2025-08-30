# Nami

A powerful, lightweight reactive framework.

[![Crates.io](https://img.shields.io/crates/v/nami)](https://crates.io/crates/nami)
[![Docs.rs](https://docs.rs/nami/badge.svg)](https://docs.rs/nami)

## Core of our architecture: `Signal` trait

```rust
use nami::watcher::{Context, WatcherGuard};

pub trait Signal: Clone + 'static {
    type Output;

    // Get the current value
    fn get(&self) -> Self::Output;

    // Register a watcher to be notified of changes
    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> impl WatcherGuard;
}
```

`Signal` describes a reactive value that can be computed and observed. It can generate a new value by `get()` method.
And it will notify watchers only when its value actually changes.

This trait is implemented by `Binding`, `Computed`, and all other reactive types, providing a consistent interface for working with reactive values regardless of their specific implementation.

`Computed<T>` is a type-erased container that can hold any implementation of the `Signal` trait, providing a uniform way to work with different kinds of computations.

### Binding

`Binding<T>` is a two-way binding container.

```rust
use nami::binding;

// Create a binding with an initial value
let counter = binding(0);

// Modify the binding
counter.set(5);
counter.increment(1); // Now equals 6

// Read the current value
assert_eq!(counter.get(), 6);
```

Bindings serve as the source of truth for application state and notify observers when their values change. They provide specialized methods for different data types:

- `Binding<bool>` - `toggle()` for boolean values
- `Binding<i32>` - `increment()`, `decrement()` for integers
- `Binding<Str>` - `append()`, `clear()` for strings
- `Binding<Vec<T>>` - `push()`, `clear()` for vectors

### Watchers

Watchers let you react to changes in reactive values:

```rust
use nami::{binding, Signal, watcher::Context};

let name = binding("World".to_string());

// Watch for changes and execute a callback
let _guard = name.watch(|ctx: Context<String>| {
    println!("Hello, {}!", ctx.value);
});

// This will trigger the watcher
name.set("Universe".to_string());
```

What's more, watchers can receive metadata through the `Context` parameter. This is essential for our reactive animation system.

When working with watchers, it's important to store the returned `WatcherGuard`. This guard ensures the watcher is properly unregistered when dropped, preventing memory leaks.
