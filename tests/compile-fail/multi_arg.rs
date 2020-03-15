#![feature(proc_macro_hygiene)]

// error-pattern:panic

extern crate threads_macro;

use threads_macro::threads;
use std::sync::{Arc, Mutex};

fn acquire(l1: &Arc<Mutex<u64>>, l2: &Arc<Mutex<u64>>) {
	let lock1 = l1.lock().unwrap();
	let lock2 = l2.lock().unwrap();
}

fn main() {
    threads!({locks = {a(0 as u64), b(0), c(0)}}, {
        acquire(&a, &b);
    }, {
		acquire(&b, &a);
    });
}
