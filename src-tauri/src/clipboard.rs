use crate::error::{Error, Result};
use clipboard_rs::{Clipboard, ClipboardContent, ClipboardContext};
use std::sync::Mutex;

#[derive(Default)]
pub struct SystemClipboard(Mutex<Option<ClipboardContext>>);

impl SystemClipboard {
    pub fn set(&self, contents: Vec<ClipboardContent>) -> Result<()> {
        let mut slot = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let context = match slot.take() {
            Some(context) => context,
            None => ClipboardContext::new().map_err(|error| Error::Clipboard(error.to_string()))?,
        };
        let result = context.set(contents);
        *slot = Some(context);
        result.map_err(|error| Error::Clipboard(error.to_string()))
    }
}
