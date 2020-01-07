#![feature(proc_macro_hygiene)]

// error-pattern:panic

extern crate threads_macro;

use threads_macro::threads;
use std::thread;
use std::sync::{Arc, Mutex};

fn main() {
    threads!({locks = {a(0), b(0), c(0)}}, {
        if true {
            a.lock().unwrap();
            b.lock().unwrap();

            println!("{}", text);
            println!("this is a test!");
        } else {
            a.lock().unwrap();
            a.lock().unwrap();
        }
    });
}