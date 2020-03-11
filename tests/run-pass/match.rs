#![feature(proc_macro_hygiene)]

extern crate threads_macro;

use threads_macro::threads;

fn main() {
    threads!({locks = {a(0 as u64), b(0), c(0)}}, {
        a.lock().unwrap();
        match Some(0) {
            Some(_) => {
                let mut temp = b.lock().unwrap();
                *temp += 10;
            },
            None => {
                let mut temp = c.lock().unwrap();
                *temp += 10;
            },
        }
    });

}

