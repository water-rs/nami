//! Compile-fail coverage for the `#[diagnostic::on_unimplemented]`
//! attributes on nami's first-contact traits.

#[test]
fn on_unimplemented_diagnostics() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/on_unimplemented/*.rs");
}

/// `List` mutators need `&mut self`: an `Fn` watcher cannot mutate a
/// captured list.
#[test]
fn list_mutation_needs_mut() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/list_mut/*.rs");
}
