use core::{any::Any, cell::RefCell};

use alloc::rc::Rc;

use crate::{
    Signal,
    watcher::{Context, WatcherGuard},
};

#[derive(Debug, Clone)]
pub struct Cached<C>
where
    C: Signal,
    C::Output: Clone,
{
    source: C,
    cache: Rc<RefCell<Option<C::Output>>>,
    _guard: Rc<dyn Any>,
}

impl<C> Cached<C>
where
    C: Signal,
    C::Output: Clone,
{
    pub fn new(source: C) -> Self {
        let cache: Rc<RefCell<Option<C::Output>>> = Rc::default();
        let guard = {
            let cache = cache.clone();
            source.watch(move |context: Context<C::Output>| {
                let value = context.value;
                *cache.borrow_mut() = Some(value);
            })
        };

        Self {
            source,
            cache,
            _guard: Rc::new(guard),
        }
    }
}

impl<C> Signal for Cached<C>
where
    C: Signal,
    C::Output: Clone,
{
    type Output = C::Output;
    fn get(&self) -> Self::Output {
        let mut cache = self.cache.borrow_mut();
        if let Some(ref cached_value) = *cache {
            cached_value.clone()
        } else {
            let value = self.source.get();
            *cache = Some(value.clone());
            value
        }
    }

    fn watch(&self, watcher: impl crate::watcher::Watcher<Self::Output>) -> impl WatcherGuard {
        self.source.watch(watcher)
    }
}

pub fn cached<C>(source: C) -> Cached<C>
where
    C: Signal,
    C::Output: Clone,
{
    Cached::new(source)
}
