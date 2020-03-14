#![feature(proc_macro_hygiene)]

// error-pattern:panic

extern crate threads_macro;

use threads_macro::threads;

fn main() {
    threads!({locks = {a(0), b(0), c(0)}}, {
        if true {
            let test1 = a.lock().unwrap();
            let test2 = b.lock().unwrap();

            println!("{}", text);
            println!("this is a test!");
        } else {
            let test1 = a.lock().unwrap();
            let test2 = a.lock().unwrap();
        }
    });
}