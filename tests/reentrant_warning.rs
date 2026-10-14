//! Debug builds warn when a `List` is mutated from inside its own watcher.

use std::sync::Mutex;

use nami::SelfId;
use nami::collection::{Collection, List};

static LOGGED: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        if record.level() == log::Level::Warn {
            LOGGED.lock().unwrap().push(record.args().to_string());
        }
    }
    fn flush(&self) {}
}

fn reentrant_warnings() -> usize {
    LOGGED
        .lock()
        .unwrap()
        .iter()
        .filter(|message| message.contains("mutated from inside one of its own watchers"))
        .count()
}

#[test]
fn reentrant_mutation_warns_in_debug_builds_only() {
    log::set_logger(&Capture).unwrap();
    log::set_max_level(log::LevelFilter::Warn);

    let mut list: List<SelfId<i32>> = List::new();
    let _guard = list.watch(.., {
        let handle = list.clone();
        move |items: nami::watcher::Context<&[SelfId<i32>]>,
              _change: &nami::collection::CollectionChange| {
            if <[_]>::len(items.value()) == 1 {
                handle.clone().push(SelfId::new(2));
            }
        }
    });

    list.push(SelfId::new(1)); // not reentrant
    assert_eq!(list.len(), 2, "the reentrant push still happened");
    let expected = usize::from(cfg!(debug_assertions));
    assert_eq!(reentrant_warnings(), expected);

    // An ordinary mutation outside any watcher does not warn.
    list.push(SelfId::new(3));
    assert_eq!(reentrant_warnings(), expected);
}
