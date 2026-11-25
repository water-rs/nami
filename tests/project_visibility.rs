//! Encapsulation tests for `#[derive(Project)]`.

use nami::{Binding, Signal, binding};

mod accounts {
    use nami::{Binding, Project, Signal, binding};

    #[derive(Clone, Debug, Project)]
    #[non_exhaustive]
    pub struct Account {
        pub name: String,
        token: String,
    }

    impl Account {
        #[must_use]
        pub fn new(name: &str, token: &str) -> Self {
            Self {
                name: name.to_owned(),
                token: token.to_owned(),
            }
        }

        #[must_use]
        pub fn token(&self) -> &str {
            &self.token
        }
    }

    #[derive(Clone, Debug, Project)]
    pub struct SecretPair(pub u32, String);

    impl SecretPair {
        #[must_use]
        pub fn new(public: u32, secret: &str) -> Self {
            Self(public, secret.to_owned())
        }

        #[must_use]
        pub fn secret(&self) -> &str {
            &self.1
        }
    }

    #[derive(Clone, Debug, Project)]
    struct PrivateOnly {
        value: i32,
    }

    #[test]
    fn private_struct_projects_within_its_module() {
        let local: Binding<PrivateOnly> = binding(PrivateOnly { value: 1 });
        let projected = local.project();
        projected.value.set(2);
        assert_eq!(local.snapshot().value, 2);
    }
}

#[test]
fn public_fields_remain_projectable_outside_defining_module() {
    let account: Binding<accounts::Account> = binding(accounts::Account::new("Ada", "secret"));
    let projected = account.project();
    projected.name.set("Grace".to_owned());
    assert_eq!(account.snapshot().name, "Grace");
    assert_eq!(account.snapshot().token(), "secret");

    let pair: Binding<accounts::SecretPair> = binding(accounts::SecretPair::new(10, "hidden"));
    let projected_pair = pair.project();
    projected_pair.0.set(20);
    assert_eq!(pair.snapshot().0, 20);
    assert_eq!(pair.snapshot().secret(), "hidden");
}

#[test]
fn private_projected_fields_fail_to_compile_outside_defining_module() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/project_visibility/*.rs");
}
