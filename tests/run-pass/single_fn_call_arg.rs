#![feature(proc_macro_hygiene)]

extern crate threads_macro;

use threads_macro::threads;

fn increment_ten(value: &mut u64) {
	*value += 10;
}

fn main() {
    threads!({locks = {a(0 as u64), b(0), c(0)}}, {
        increment_ten(&mut *a.lock().unwrap());
    }, {
        let mut data = a.lock().unwrap();
        increment_ten(&mut *data);
    });
}
