use std::sync::{Mutex, Arc};

use valuable::{NamedValues, Value};

// TODO: Set visibility level to `crate`
#[derive(Debug, Default, Clone)]
pub struct NamedValues_<'a>(pub Arc<Mutex<NamedValues<'a>>>);

impl<'a> NamedValues_<'a> {
    pub fn new(named_values: NamedValues<'a>) -> Self {
        Self(Arc::new(Mutex::new(named_values)))
    }

    pub fn new_from_ref(named_values: &'a NamedValues<'a>) -> Self {
        Self(Arc::new(Mutex::new(*named_values)))
    }

    pub fn is_empty(&self) -> bool {
        self.0.lock().unwrap().is_empty()
    }

}

impl<'a> PartialEq for NamedValues_<'a> {
    fn eq(&self, _other: &Self) -> bool {
        todo!();
    }
}

impl<'a> Eq for NamedValues_<'a> {}