//! Compile-fail coverage for the `#[diagnostic::on_unimplemented]`
//! attributes on nami's first-contact traits.

#[test]
fn on_unimplemented_diagnostics() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/on_unimplemented/*.rs");
}
