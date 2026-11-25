//! Private fields on a `#[derive(Project)]` struct must stay private on its
//! generated `*Projected` struct.

mod inner {
    use nami::Project;

    #[derive(Clone, Project)]
    pub struct Account {
        pub name: String,
        token: String,
    }

    impl Account {
        pub fn new() -> Self {
            Self {
                name: "Ada".to_owned(),
                token: "secret".to_owned(),
            }
        }
    }
}

fn main() {
    let account: nami::Binding<inner::Account> = nami::binding(inner::Account::new());
    account.project().token.set("forged".to_owned());
}
