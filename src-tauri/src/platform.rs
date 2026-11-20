// The web build had shims for the app handle and async runtime; on Tauri both are real.
use std::future::Future;

pub use tauri::AppHandle;

pub fn spawn(task: impl Future<Output = ()> + Send + 'static) {
    tauri::async_runtime::spawn(task);
}

pub async fn sleep(duration: std::time::Duration) {
    tokio::time::sleep(duration).await;
}

pub fn log(message: &str) {
    eprintln!("{message}");
}
