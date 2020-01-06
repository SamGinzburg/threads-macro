#![feature(proc_macro_hygiene)]

extern crate compiletest_rs as compiletest;

use std::path::PathBuf;
use compiletest_rs::common::Mode::*;

fn run_mode(mode: &'static str) {
	let mut config = compiletest::Config::default();
	//config.verbose = true;
    let cfg_mode = mode.parse().ok().expect("Invalid mode");

    config.mode = cfg_mode;
    config.src_base = PathBuf::from(format!("tests/{}", mode));
    config.target_rustcflags = Some("-L target/debug -L target/debug/deps".to_string());

    compiletest::run_tests(&config);
}

#[test]
fn compile_fail() {
    run_mode("compile-fail");
}

#[test]
fn run_pass() {
    run_mode("run-pass");
}