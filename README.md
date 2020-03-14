# Automatic deadlock detection for Rust programs

This macro utilizes Rust's type system to provide a DSL that guarentees the absence of all deadlocks for a set of statically declared locks at compile time. The DSL is a significant subset of the safe Rust language, and should be easy to use for existing Rust programmers.

I would like to emphasize that for now, this project is a WIP and not fully complete yet. There are likely bugs / missing features as well as a lack of any clear formalization for now.

# Why?

The primary existing deadlock detection tool for Rust code (parking_lot crate) checks for deadlocks dynamically as opposed to statically. There are many deadlock detection tools for C++/C available as well, although they operate dynamically as well. Static analysis tools such as clang/coverity exist, but they do not guarentee the absence of deadlocks.

In this crate and project, my primary aim is to demonstate the versitility of the properties provided by the Rust type system. I aim to show that Rust's type system can be used to provide properties not previously considered with minor syntactical restrictions.

This crate aims to provide the following properties:
1) Static deadlock detection for a static lock set
2) Support inter-operation of safe Rust code with the DSL
3) Easy to use in production / real systems

These goals are accomplished by utilizing the semantics provided by Rust's ownership and borrowing mechanisms, combined with the linear regions used for memory management, to perform static deadlock analysis that is complete. The deadlock detection and safe inter-operation with code outside of the macro is made possible by these properties.

# Syntax

The macro's DSL has two main components to it. First, a set of locks must be declared with the corresponding data they protect. Each lock is automatically made available to each code block.

Then each consecutive code block is written in a DSL - which is a signficant subset of the safe Rust language. The domain specific language makes certain syntax restrictions to check for deadlocks. The following restrictions are made:

- When calling functions at most 1 lock reference can be passed as a function parameter
- New mutexes cannot be created
- Existing mutexes cannot be assigned to new local bindings (no copying of lock references)
- No unsafe code blocks are permitted within the macro
- No locks with non-whitelisted identifiers may be acquired
- Indirect function calls are not permitted (ex: let fn = "123".parse(); fn(_))

Rust procedural macros have some limitations on what they can do, so we also must make the following assumptions:

- Functions called within the DSL exclusively contain safe Rust code
- Nested procedural macros may not acquire or release locks 

# Examples

Here is an example of some code that performs some basic arithmetic. This code obviously didn't need this macro to be correct, but it is a basic example of how locks can be acquired/used.

```rust
#![feature(proc_macro_hygiene)]

extern crate threads_macro;

use threads_macro::threads;

fn main() {
    threads!({locks = {a(0)}}, {
        for _ in 0..1000 {
            let mut data = a.lock().unwrap();
            *data += 100;
        }
    }, {
        for _ in 0..1000 {
            let mut data = a.lock().unwrap();
            *data += 100;
        }
    });
    println!("{}", a.lock().unwrap());
}
```

Here is another example of the interoperation between the DSL and external safe Rust code:
```rust
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
```

Additional examples are provided within the compile-fail and run-pass subdirectories in the 'tests' directory. Test cases in the run-pass subdirectory are examples of correct code with no deadlocks, and test cases in the compile-fail subdirectory are examples of potential deadlocks that can be detected with this system.
