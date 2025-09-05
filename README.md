# Nami

A powerful, lightweight reactive framework for Rust.

[![Crates.io](https://img.shields.io/crates/v/nami)](https://crates.io/crates/nami)
[![Docs.rs](https://docs.rs/nami/badge.svg)](https://docs.rs/nami)

- `no_std` with `alloc`
- Consistent `Signal` trait across computed values
- Ergonomic two-way `Binding<T>` with helpers
- Composition primitives: `map`, `zip`, `cached`, `debounce`, `throttle`, `utils::{add,max,min}`
- Typed watcher context with metadata
- Optional derive macros and string signal macro `s!`

## Quick Start

```rust
use nami::{binding, Binding, Signal};

// Create mutable reactive state
let counter: Binding<i32> = binding(0);

// Derive a new computation from it
let doubled = nami::map::map(counter.clone(), |n: i32| n * 2);

// Read current values
assert_eq!(counter.get(), 0);
assert_eq!(doubled.get(), 0);

// Update the source and observe derived changes
counter.set(3);
assert_eq!(doubled.get(), 6);
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

`Binding<T>` is two-way reactive state with ergonomic helpers.

```rust
use nami::{binding, Binding};

let counter: Binding<i32> = binding(0);
counter.set(5);
counter.increment(1);
assert_eq!(counter.get(), 6);
```

Common helpers:

- `Binding<bool>`: `toggle()`, `then(...)`, `select(a,b)`
- `Binding<i32>`: `increment(n)`, `decrement(n)`
- `Binding<String>`: `append(...)`, `clear()`
- `Binding<Vec<T>>`: `push(...)`, `insert(...)`, `pop()`, `clear()`

## Watchers

React to changes via `watch`. Keep the returned guard alive to stay subscribed.

```rust,no_run
use nami::{binding, Signal, watcher::Context};

let name: Binding<String> = binding("World".to_string());

let _guard = name.watch(|ctx: Context<String>| {
    // metadata is available via ctx.metadata
    // println! is just an example side-effect
    println!("Hello, {}!", ctx.value);
});

name.set("Universe".to_string());
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
use nami::{binding, debounce::Debounce, throttle::Throttle};
use core::time::Duration;

let input = binding("".to_string());

// Debounce: delay updates until 300ms of quiet time
let debounced = Debounce::new(input.clone(), Duration::from_millis(300));

// Throttle: limit to at most one update per 100ms
let throttled = Throttle::new(input.clone(), Duration::from_millis(100));

// Both preserve reactivity while controlling update frequency
input.set("typing...".to_string());
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

```rust,ignore
use nami::{Signal, stream::SignalStream};
// let s = /* some Signal */;
// let mut stream = SignalStream { signal: s, guard: None };
// while let Some(value) = stream.next().await { /* ... */ }
```

**Enhanced Mailboxes** (requires `native-executor` feature):

```rust,no_run
use nami::binding;
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

Enable structured logging of computations and changes:

```rust,no_run
use nami::{Signal, SignalExt, debug::{Debug, Config, ConfigFlags}};

let config = Config { flags: ConfigFlags::CHANGE };
let debugged = Debug::with_config(123_i32.computed(), config);
let _ = debugged.get();
```

## Derive Macros and `s!` Macro

Enable the `derive` feature (enabled by default) to access:

- `#[derive(nami::Project)]`: project a struct binding into bindings for each field
- `s!("Hello {name}")`: string formatting that captures variables as signals

```rust
use nami::{binding, Binding, project::Project};

#[derive(Clone, nami::Project)]
struct Person { name: String, age: u32 }

let p: Binding<Person> = binding(Person { name: "A".into(), age: 1 });
// The derive generates `PersonProjected`
let projected: PersonProjected = p.project();
projected.name.set("B".into());
assert_eq!(p.get().name, "B");
```

## Installation

```toml
[dependencies]
nami = { version = "0.3", features = ["derive", "native-executor"] }
```

Feature flags:

- `derive` (default): re-exports macros from `nami-derive`
- `native-executor` (default): integrates with `native-executor` for mailbox helpers

## Notes

- `no_std`: the crate is `#![no_std]` and uses `alloc`.
- Keep watcher guards alive to remain subscribed; dropping the guard unsubscribes.
- Many examples are `no_run` because they require an executor or side effects.
