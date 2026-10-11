//! Unit-test app lifetimes own the process-wide GPU preference. Multiple apps in
//! one test may nest, but another test must not change that preference mid-render.
use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard};
static GPU: Mutex<()> = Mutex::new(());
thread_local! {
    static HELD: RefCell<Option<(MutexGuard<'static, ()>, usize)>> = const { RefCell::new(None) };
}
pub struct Guard(PhantomData<Rc<()>>);
pub fn lock() -> Guard {
    HELD.with(|held| {
        let mut held = held.borrow_mut();
        if let Some((_, depth)) = held.as_mut() {
            *depth += 1;
        } else {
            *held = Some((GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner), 1));
        }
    });
    Guard(PhantomData)
}
impl Drop for Guard {
    fn drop(&mut self) {
        HELD.with(|held| {
            let mut held = held.borrow_mut();
            if let Some((_, depth)) = held.as_mut() {
                *depth -= 1;
                if *depth == 0 {
                    *held = None;
                }
            }
        });
    }
}
