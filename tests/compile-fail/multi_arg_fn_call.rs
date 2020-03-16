#![feature(proc_macro_hygiene)]

// error-pattern:panic

extern crate threads_macro;

use threads_macro::threads;

fn increment_ten(value: &mut u64, value2: u64) {
	*value += value2;
}

fn main() {
    threads!({locks = {a(0 as u64), b(0), c(0)}}, {
        increment_ten(&mut *a.lock().unwrap(), &*b.lock().unwrap());
    }, {
        let mut data = a.lock().unwrap();
        let mut data2 = b.lock().unwrap();
        increment_ten(&mut *data, data2);
    });
}
