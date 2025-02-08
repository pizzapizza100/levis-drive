// main.rs

use log::{debug, error, info, warn};
use std::process;

fn main() {
    std::env::set_var("RUST_LOG", "debug");
    env_logger::init(); // Initialize the logger implementation

    debug!("This is a debug message");
    info!("This is an info message");
    warn!("This is an warning message");
    error!("This is an error message");

    process::exit(0);
}

#[test]
fn always_pass_test() {
    assert_eq!(1, 1);
}
