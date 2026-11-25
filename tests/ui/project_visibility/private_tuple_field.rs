//! Private fields on a `#[derive(Project)]` tuple struct must stay private on
//! its projection.

mod inner {
    use nami::Project;

    #[derive(Clone, Project)]
    pub struct Secret(pub u32, String);

    impl Secret {
        pub fn new() -> Self {
            Self(1, "secret".to_owned())
        }
    }
}

fn main() {
    let secret: nami::Binding<inner::Secret> = nami::binding(inner::Secret::new());
    secret.project().1.set("forged".to_owned());
}
