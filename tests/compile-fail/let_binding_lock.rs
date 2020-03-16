#![feature(proc_macro_hygiene)]

// error-pattern:panic

extern crate threads_macro;

use threads_macro::threads;
use std::thread::sleep;

fn lock_clone(a: &Arc<Mutex<u64>>) -> Arc<Mutex<u64>> {
    a.clone()
}

fn main() {
    threads!({locks = {a(0)}}, {
        let test1 = lock_clone(&a);
        let test2 = test1.lock().unwrap();
        let test3 = a.lock().unwrap();
    });
}