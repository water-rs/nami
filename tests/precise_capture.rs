//! Signals derived through borrowing `SignalExt` methods hold clones of
//! their source, so they outlive the binding they were derived from.

use nami::{Binding, Signal, SignalExt, zip::FlattenMap};

#[expect(
    clippy::needless_pass_by_value,
    reason = "the binding is dropped on return; the derived signal must outlive it"
)]
fn positive(count: Binding<u64>) -> impl Signal<Output = bool> {
    count.gt(0)
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "the binding is dropped on return; the derived signal must outlive it"
)]
fn label(count: Binding<u64>) -> impl Signal<Output = &'static str> {
    count.equal_to(0).select("none", "some")
}

fn sum(a: Binding<u64>, b: Binding<u64>) -> impl Signal<Output = u64> {
    nami::zip::zip(a, b).flatten_map(|a, b| a + b)
}

#[test]
fn a_derived_signal_outlives_its_source_binding() {
    assert!(positive(Binding::u64(1)).snapshot());
    assert_eq!(label(Binding::u64(0)).snapshot(), "none");
    assert_eq!(sum(Binding::u64(2), Binding::u64(3)).snapshot(), 5);
}
