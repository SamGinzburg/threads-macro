#![feature(proc_macro_hygiene)]

// error-pattern:panic

extern crate threads_macro;

use threads_macro::threads;
use std::thread::sleep;

fn main() {
	/*

		5 person table

		person 1: a, b
		person 2: b, c
		person 3: c, d
		person 4: d, e
		person 5: e, a

	 */
    threads!({locks = {a(0), b(0), c(0), d(0), e(0)}}, {
       loop {
		   println!("think");
		   a.lock().unwrap();
		   b.lock().unwrap();
		   println!("eat");
		   sleep(1000);
	   }
	}, {
		loop {
			println!("think");
			b.lock().unwrap();
			c.lock().unwrap();
			println!("eat");
			sleep(1000);
		}
	}, {
		loop {
			println!("think");
			c.lock().unwrap();
			d.lock().unwrap();
			println!("eat");
			sleep(1000);
		}
	}, {
		loop {
			println!("think");
			d.lock().unwrap();
			e.lock().unwrap();
			println!("eat");
			sleep(1000);
		}
	}, {
		loop {
			println!("think");
			e.lock().unwrap();
			a.lock().unwrap();
			println!("eat");
			sleep(1000);
		}
	});
}