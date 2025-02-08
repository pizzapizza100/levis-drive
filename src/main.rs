// main.rs

use log::{debug, error, info};

fn main() {
    std::env::set_var("RUST_LOG", "debug");
    env_logger::init(); // Initialize the logger implementation

    info!("This is an info message");
    debug!("This is a debug message");
    error!("This is an error message");
}

#[test]
fn always_pass_test() {
    assert_eq!(1, 1);
}
