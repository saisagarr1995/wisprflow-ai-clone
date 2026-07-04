use std::sync::{Arc, Mutex};

use crate::{InjectError, Injector};

/// Records injected text for pipeline tests; optionally fails on demand.
#[derive(Clone, Default)]
pub struct FakeInjector {
    pub injected: Arc<Mutex<Vec<String>>>,
    pub fail_next: Arc<Mutex<bool>>,
}

impl Injector for FakeInjector {
    fn inject(&mut self, text: &str) -> Result<(), InjectError> {
        if std::mem::take(&mut *self.fail_next.lock().unwrap()) {
            return Err(InjectError::Synthesis("forced failure".into()));
        }
        self.injected.lock().unwrap().push(text.to_string());
        Ok(())
    }
}
