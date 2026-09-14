use nami::Signal;

fn takes<S: Signal>(_: S) {}

struct NotReactive;

fn main() {
    takes(NotReactive);
}
