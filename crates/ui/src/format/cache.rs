//! Formatters are made from locale data once per thread and reused: a list
//! of a thousand rows formats a thousand dates with a handful of formatters.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

thread_local! {
    static FORMATTERS: RefCell<HashMap<String, Rc<dyn Any>>> = RefCell::new(HashMap::new());
}

/// The formatter stored under `key`, made by `make` the first time. `None`
/// when `make` fails (the data does not cover the request), which is not
/// remembered.
pub(crate) fn cached<T: 'static>(key: &str, make: impl FnOnce() -> Option<T>) -> Option<Rc<T>> {
    if let Some(found) = FORMATTERS.with(|map| map.borrow().get(key).cloned()) {
        return found.downcast::<T>().ok();
    }
    let made: Rc<T> = Rc::new(make()?);
    FORMATTERS.with(|map| {
        let mut map = map.borrow_mut();
        // A run of unbounded keys (a currency typed by a person) must not
        // grow the map for ever.
        if map.len() > 512 {
            map.clear();
        }
        map.insert(key.to_string(), made.clone() as Rc<dyn Any>);
    });
    Some(made)
}
