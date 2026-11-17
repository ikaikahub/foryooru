use serde::Serialize;
use std::sync::{Mutex, MutexGuard};
use crate::platform::AppHandle;

pub struct Progress<T> {
    status: Mutex<T>,
    event: &'static str,
}

impl<T: Clone + Default + Serialize> Progress<T> {
    pub fn new(event: &'static str) -> Progress<T> {
        Progress {
            status: Mutex::new(T::default()),
            event,
        }
    }

    fn lock(&self) -> MutexGuard<'_, T> {
        self.status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn get(&self) -> T {
        self.lock().clone()
    }

    pub fn update(&self, app: &AppHandle, change: impl FnOnce(&mut T)) {
        let status = {
            let mut status = self.lock();
            change(&mut status);
            status.clone()
        };
        app.emit(self.event, status);
    }
}
