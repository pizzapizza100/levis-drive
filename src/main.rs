mod posted_ip;

use dotenv::dotenv;
use std::fmt::Error;

fn main() -> Result<(), Error> {
    dotenv().ok();
    env_logger::init(); // Initialize the logger implementation

    posted_ip::verify_posted_ip().unwrap();

    Ok(())
}
