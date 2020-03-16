#![feature(proc_macro_hygiene)]

// error-pattern:panic

extern crate threads_macro;

use threads_macro::threads;
use std::thread::sleep;

fn main() {
	threads!({locks = {a(0), b(0), c(0), d(0), e(0)}}, {
		let test = a.clone();
		let test1 = test.lock().unwrap();
		let test2 = a.lock().unwrap();
	});
}