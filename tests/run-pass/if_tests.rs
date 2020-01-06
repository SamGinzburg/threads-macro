#![feature(proc_macro_hygiene)]

extern crate threads_macro;

use threads_macro::threads;
use std::sync::{Arc, Mutex};
use std::thread;

fn main() {
    threads!({locks = {a, b, c}}, {
        if true {
            a.lock().unwrap();
            b.lock().unwrap();
        } else {
            a.lock().unwrap();
            b.lock().unwrap();
        }
    }, {
        if true {
            a.lock().unwrap();
            b.lock().unwrap();
        } else {
            a.lock().unwrap();
            b.lock().unwrap();
        }
	});
}

