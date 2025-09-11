# Nami

A powerful, lightweight reactive framework for Rust.

[![Crates.io](https://img.shields.io/crates/v/nami)](https://crates.io/crates/nami)
[![Docs.rs](https://docs.rs/nami/badge.svg)](https://docs.rs/nami)

- `no_std` with `alloc`
- Consistent `Signal` trait across computed values
- Ergonomic two-way `Binding<T>` with helpers
- Composition primitives: `map`, `zip`, `cached`, `debounce`, `throttle`, `utils::{add,max,min}`
- Typed watcher context with metadata
- Optional derive macros

## Quick Start

```rust
use nami::{binding, Binding, Signal};

// Create mutable reactive state with automatic type conversion
let counter: Binding<i32> = binding(0);
let message: Binding<String> = binding("hello");  // &str -> String conversion

// Derive a new computation from it
let doubled = nami::map::map(counter.clone(), |n: i32| n * 2);

// Read current values
assert_eq!(counter.get(), 0);
assert_eq!(doubled.get(), 0);

// Update the source and observe derived changes
counter.set(3);
assert_eq!(doubled.get(), 6);

// set() also accepts Into<T> for ergonomic updates
message.set("world");  // &str works directly!
```

## The `Signal` Trait

All reactive values implement a single trait:

```rust
use nami::watcher::{Context, WatcherGuard};

pub trait Signal: Clone + 'static {
    type Output;
    fn get(&self) -> Self::Output;
    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> impl WatcherGuard;
}
```

- `get`: compute and return the current value.
- `watch`: subscribe to changes; returns a guard. Drop the guard to unsubscribe.

`Binding`, `Computed`, and all adapters implement `Signal` so you can compose them freely.

## Bindings

`Binding<T>` is two-way reactive state with ergonomic helpers. Both `binding()` and `set()` accept any value implementing `Into<T>`, eliminating the need for manual conversions:

```rust
use nami::{binding, Binding};

// Automatic type conversion with Into trait
let text: Binding<String> = binding("hello");           // &str -> String
let counter: Binding<i32> = binding(0);                 // Direct initialization
let items: Binding<Vec<i32>> = binding(vec![1, 2, 3]);  // Vec<i32> binding

// set() also uses Into<T> for ergonomic updates
text.set("world");                                // Direct &str, no .into() needed
counter.set(5);
counter.increment(1);
assert_eq!(counter.get(), 6);

// Works with type conversions
let bignum: Binding<i64> = binding(0i64);
bignum.set(42i32);                               // i32 -> i64 automatic conversion
```

Common helpers:

- `Binding<bool>`: `toggle()`, `then(...)`, `select(a,b)`
- `Binding<i32>`: `increment(n)`, `decrement(n)`
- `Binding<String>`: `append(...)`, `clear()`
- `Binding<Vec<T>>`: `push(...)`, `insert(...)`, `pop()`, `clear()`

## Watchers

React to changes via `watch`. Keep the returned guard alive to stay subscribed.

```rust,no_run
use nami::{binding, Binding, Signal, watcher::Context};

let name: Binding<String> = binding("World");

let _guard = name.watch(|ctx: Context<String>| {
    // metadata is available via ctx.metadata
    // println! is just an example side-effect
    println!("Hello, {}!", ctx.value);
});

name.set("Universe");
```

The `Context` carries typed metadata to power advanced features (e.g., animations).

## Composition Primitives

- `map(source, f)`: transform values while preserving reactivity
- `zip(a, b)`: combine two signals into `(A::Output, B::Output)`
- `cached(signal)`: cache last value and avoid recomputation
- `debounce(signal, duration)`: delay updates until a quiet period
- `throttle(signal, duration)`: limit update rate to at most once per duration
- `utils::{add, max, min}`: convenient combinators built on `zip` + `map`

```rust
use nami::{binding, Binding, Signal};
use nami::{map::map, zip::zip};

let a: Binding<i32> = binding(2);
let b: Binding<i32> = binding(3);

let sum = nami::utils::add(a.clone(), b.clone());
assert_eq!(sum.get(), 5);

let pair = zip(a, b);
assert_eq!(pair.get(), (2, 3));

let squared = map(sum, |n: i32| n * n);
assert_eq!(squared.get(), 25);
```

## Rate Limiting: Debounce and Throttle

Control the rate of updates with debounce and throttle utilities:

```rust
use nami::{binding, debounce::Debounce, throttle::Throttle, Binding};
use core::time::Duration;

let input: Binding<String> = binding("");

// Debounce: delay updates until 300ms of quiet time
let debounced = Debounce::new(input.clone(), Duration::from_millis(300));

// Throttle: limit to at most one update per 100ms
let throttled = Throttle::new(input.clone(), Duration::from_millis(100));

// Both preserve reactivity while controlling update frequency
input.set("typing...");
```

**Debounce vs Throttle:**

- **Debounce**: Waits for a quiet period, useful for search input, API calls
- **Throttle**: Limits maximum update rate, useful for scroll events, animations

## Type-Erased `Computed<T>`

`Computed<T>` stores any `Signal<Output = T>` behind a stable, type-erased handle.

```rust
use nami::{Signal, SignalExt};

let c = 10_i32.computed();
assert_eq!(c.get(), 10);

let plus_one = c.map(|n| n + 1);
assert_eq!(plus_one.get(), 11);
```

## Async Interop

Bridge async with reactive using adapters:

- `FutureSignal<T>`: `Option<T>` becomes `Some(T)` when a future resolves
- `SignalStream<S>`: treat a `Signal` as a `Stream` that yields on updates
- `BindingMailbox<T>`: cross-thread reactive state with `get()`, `set()`, and `get_as()` for type conversion

```rust,no_run
use nami::future::FutureSignal;
use executor_core::LocalExecutor;

// Requires an executor; example omitted for brevity
// let sig = FutureSignal::new(executor, async { 42 });
// assert_eq!(sig.get(), None);
// ... later ... sig.get() == Some(42)
```

```rust
use nami::{Signal, stream::SignalStream};
// let s = /* some Signal */;
// let mut stream = SignalStream { signal: s, guard: None };
// while let Some(value) = stream.next().await { /* ... */ }
```

**Enhanced Mailboxes** (requires `native-executor` feature):

```rust,ignore
use nami::{binding, Binding};
use waterui_str::Str; // Example non-Send type

// Create binding with non-Send type
let text_binding:Binding<Str> = binding("hello");
let mailbox = text_binding.mailbox();

// Convert to Send type for cross-thread usage
let owned_string: String = mailbox.get_as().await;
assert_eq!(owned_string, "hello");

// Regular mailbox operations
mailbox.set("world").await;
```

## Debugging

Enable structured logging to trace signal behavior during development:

```rust,no_run
use nami::{binding, Binding, Signal, debug::{Debug, Config}};

let value: Binding<i32> = binding(42);

// Log only value changes (most common)
let debug = Debug::changes(value.clone());

// Log all operations (verbose mode)
let debug = Debug::verbose(value.clone());

// Log specific operations
let debug = Debug::compute_only(value.clone());        // Only computations
let debug = Debug::watchers(value.clone());            // Watcher lifecycle
let debug = Debug::compute_and_changes(value.clone()); // Both computations and changes

// Use custom configuration
let debug = Debug::with_config(value, Config::default());
```

The debug module uses the `log` crate for output, so configure your logger (e.g., `env_logger`) to see the debug messages.

## Derive Macros

Enable the `derive` feature (enabled by default) to access:

- `#[derive(nami::Project)]`: project a struct binding into bindings for each field

```rust
use nami::{binding, Binding, project::Project};

#[derive(Clone, nami::Project)]
struct Person { name: String, age: u32 }

let p: Binding<Person> = binding(Person { name: "A".into(), age: 1 });
// The derive generates `PersonProjected`
let projected: PersonProjected = p.project();
projected.name.set("B");  // Automatic &str -> String conversion
assert_eq!(p.get().name, "B");
```

Feature flags:

- `derive` (default): re-exports macros from `nami-derive`
- `native-executor` (default): integrates with `native-executor` for mailbox helpers

## Notes

- `no_std`: the crate is `#![no_std]` and uses `alloc`.
- Keep watcher guards alive to remain subscribed; dropping the guard unsubscribes.
- Many examples are `no_run` because they require an executor or side effects.
