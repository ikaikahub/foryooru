// rayon on native builds; the browser runs the engine on one thread, where the same calls fall back to plain iterators
#[cfg(not(target_arch = "wasm32"))]
pub use rayon::prelude::*;

#[cfg(target_arch = "wasm32")]
pub trait IntoParallelIterator: IntoIterator + Sized {
    fn into_par_iter(self) -> Self::IntoIter {
        self.into_iter()
    }
}

#[cfg(target_arch = "wasm32")]
impl<T: IntoIterator> IntoParallelIterator for T {}

#[cfg(target_arch = "wasm32")]
pub trait ParallelSlice<T> {
    fn par_iter(&self) -> std::slice::Iter<'_, T>;
    fn par_sort_unstable_by<F: FnMut(&T, &T) -> std::cmp::Ordering>(&mut self, compare: F);
}

#[cfg(target_arch = "wasm32")]
impl<T> ParallelSlice<T> for [T] {
    fn par_iter(&self) -> std::slice::Iter<'_, T> {
        self.iter()
    }

    fn par_sort_unstable_by<F: FnMut(&T, &T) -> std::cmp::Ordering>(&mut self, compare: F) {
        self.sort_unstable_by(compare);
    }
}
