//! Signals derived through borrowing `SignalExt` methods hold clones of
//! their source, so they outlive the binding they were derived from.
//!
//! Each helper takes its source binding by value, so the binding is dropped
//! when the helper returns while the derived signal it returns lives on: a
//! derived signal that captured the `&self` borrow does not compile here.

#![expect(
    clippy::needless_pass_by_value,
    reason = "each source binding is dropped on return; the derived signal must outlive it"
)]

use nami::{Binding, Signal, SignalExt, binding, zip::FlattenMap};

fn inspected(count: Binding<u64>) -> impl Signal<Output = u64> {
    count.inspect(|_| {})
}

fn even(count: Binding<u64>) -> impl Signal<Output = bool> {
    count.condition(|count| count % 2 == 0)
}

fn comparisons(count: Binding<u64>) -> impl Signal<Output = [bool; 5]> {
    let gt = count.gt(1);
    let lt = count.lt(1);
    let ge = count.ge(1);
    let le = count.le(1);
    let eq = count.equal_to(1);
    nami::zip::zip(
        nami::zip::zip(gt, lt),
        nami::zip::zip(nami::zip::zip(ge, le), eq),
    )
    .map(|((gt, lt), ((ge, le), eq))| [gt, lt, ge, le, eq])
}

fn label(count: Binding<u64>) -> impl Signal<Output = &'static str> {
    count.equal_to(0).select("none", "some")
}

fn flag(on: Binding<bool>) -> impl Signal<Output = Option<&'static str>> {
    on.then_some("on")
}

fn options(value: Binding<Option<u64>>) -> impl Signal<Output = (u64, u64, bool)> {
    let or = value.unwrap_or(7);
    let or_else = value.unwrap_or_else(|| 9);
    let is_two = value.some_equal_to(2);
    nami::zip::zip(nami::zip::zip(or, or_else), is_two)
        .flatten_map(|or, or_else, is_two| (or, or_else, is_two))
}

fn option_maps(value: Binding<Option<u64>>) -> impl Signal<Output = (Option<u64>, Option<u64>)> {
    let doubled = value.map_some(|value| value * 2);
    let halved = value.and_then_some(|value| value.is_multiple_of(2).then_some(value / 2));
    nami::zip::zip(doubled, halved)
}

fn results(
    value: Binding<Result<u64, String>>,
) -> impl Signal<Output = (u64, u64, Result<u64, String>, Result<u64, usize>)> {
    let or = value.unwrap_or_result(0);
    let or_else = value.unwrap_or_else_result(|error| error.len() as u64);
    let ok = value.map_ok(|value| value + 1);
    let err = value.map_err(|error| error.len());
    nami::zip::zip(nami::zip::zip(or, or_else), nami::zip::zip(ok, err))
        .map(|((or, or_else), (ok, err))| (or, or_else, ok, err))
}

fn mentions(text: Binding<String>, word: &str) -> impl Signal<Output = bool> + use<> {
    text.str_contains(word)
}

fn sum(a: Binding<u64>, b: Binding<u64>) -> impl Signal<Output = u64> {
    nami::zip::zip(a, b).flatten_map(|a, b| a + b)
}

fn sum3(a: Binding<u64>, b: Binding<u64>, c: Binding<u64>) -> impl Signal<Output = u64> {
    nami::zip::zip(nami::zip::zip(a, b), c).flatten_map(|a, b, c| a + b + c)
}

#[test]
fn a_derived_signal_outlives_its_source_binding() {
    assert_eq!(inspected(binding(4u64)).snapshot(), 4);
    assert!(even(binding(4u64)).snapshot());
    assert_eq!(
        comparisons(binding(1u64)).snapshot(),
        [false, false, true, true, true]
    );
    assert_eq!(label(binding(0u64)).snapshot(), "none");
    assert_eq!(flag(binding(true)).snapshot(), Some("on"));
    assert_eq!(options(binding(Some(2u64))).snapshot(), (2, 2, true));
    assert_eq!(options(binding(None::<u64>)).snapshot(), (7, 9, false));
    assert_eq!(
        option_maps(binding(Some(4u64))).snapshot(),
        (Some(8), Some(2))
    );
    assert_eq!(
        results(binding(Err::<u64, String>("lost".into()))).snapshot(),
        (0, 4, Err("lost".into()), Err(4))
    );
    assert_eq!(
        results(binding(Ok::<u64, String>(3))).snapshot(),
        (3, 3, Ok(4), Ok(3))
    );
    assert!(mentions(binding(String::from("pending review")), "review").snapshot());
    assert_eq!(sum(binding(2u64), binding(3u64)).snapshot(), 5);
    assert_eq!(
        sum3(binding(1u64), binding(2u64), binding(3u64)).snapshot(),
        6
    );
}
