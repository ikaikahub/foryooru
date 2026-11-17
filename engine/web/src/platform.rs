// What Tauri provided on the desktop, done the browser's way. The engine runs on one Web Worker thread,
// so settings and the event callback live in thread-locals and background work uses spawn_local.
use serde::Serialize;
use serde_json::{Map, Value};
use std::cell::RefCell;
use std::future::Future;
use std::time::Duration;

thread_local! {
    static SETTINGS: RefCell<Map<String, Value>> = RefCell::new(Map::new());
    static EMIT: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
}

// stands in for tauri::AppHandle, which the copied modules pass around
#[derive(Clone, Copy, Default)]
pub struct AppHandle;

impl AppHandle {
    pub fn setting(&self, key: &str) -> Option<Value> {
        SETTINGS.with(|settings| settings.borrow().get(key).cloned())
    }

    pub fn emit(&self, event: &str, payload: impl Serialize) {
        EMIT.with(|emit| {
            let Some(emit) = emit.borrow().clone() else {
                return;
            };
            let payload = serde_wasm_bindgen::to_value(&payload).unwrap_or(wasm_bindgen::JsValue::NULL);
            if emit.call2(&wasm_bindgen::JsValue::NULL, &event.into(), &payload).is_err() {
                log(&format!("Couldn't report {event}"));
            }
        });
    }
}

// the page's settings store is the source of truth, it sends a fresh copy whenever something changes
pub fn replace_settings(settings: Map<String, Value>) {
    SETTINGS.with(|current| *current.borrow_mut() = settings);
}

pub fn set_emitter(emit: js_sys::Function) {
    EMIT.with(|current| *current.borrow_mut() = Some(emit));
}

pub fn spawn(task: impl Future<Output = ()> + 'static) {
    wasm_bindgen_futures::spawn_local(task);
}

pub async fn sleep(duration: Duration) {
    gloo_timers::future::sleep(duration).await;
}

pub fn log(message: &str) {
    web_sys_console_log(message);
}

#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = warn)]
    fn web_sys_console_log(message: &str);
}
