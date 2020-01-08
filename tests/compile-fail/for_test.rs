#![feature(proc_macro_hygiene)]

// error-pattern:panic

extern crate threads_macro;

use threads_macro::threads;

fn main() {
    threads!({locks = {a(0), b(0), c(0)}}, {
        for _ in 0..1000 {
            let mut data = a.lock().unwrap();
            *data += 100;
        }
    }, {
        for _ in 0..1000 {
            let mut data = a.lock().unwrap();
            let data2 = a.lock().unwrap();
            *data += 100;
        }
    });
    println!("{}", a.lock().unwrap());
}
