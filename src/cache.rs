use core::{any::Any, cell::RefCell};

use alloc::rc::Rc;

use crate::{Compute, watcher::WatcherGuard};

#[derive(Debug, Clone)]
pub struct Cached<C>
where
    C: Compute,
    C::Output: Clone,
{
    source: C,
    cache: Rc<RefCell<Option<C::Output>>>,
    _guard: Rc<dyn Any>,
}

impl<C> Cached<C>
where
    C: Compute,
    C::Output: Clone,
{
    pub fn new(source: C) -> Self {
        let cache: Rc<RefCell<Option<C::Output>>> = Rc::default();
        let guard = {
            let cache = cache.clone();
            source.add_watcher(move |value, _| {
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

impl<C> Compute for Cached<C>
where
    C: Compute,
    C::Output: Clone,
{
    type Output = C::Output;
    fn compute(&self) -> Self::Output {
        let mut cache = self.cache.borrow_mut();
        if let Some(ref cached_value) = *cache {
            cached_value.clone()
        } else {
            let value = self.source.compute();
            *cache = Some(value.clone());
            value
        }
    }

    fn add_watcher(
        &self,
        watcher: impl crate::watcher::Watcher<Self::Output>,
    ) -> impl WatcherGuard {
        self.source.add_watcher(watcher)
    }
}

pub fn cached<C>(source: C) -> Cached<C>
where
    C: Compute,
    C::Output: Clone,
{
    Cached::new(source)
}
