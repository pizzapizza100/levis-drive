use log::debug;
use reqwest::blocking::get;
use reqwest::Error;

fn get_router_public_ip() -> Result<String, Error> {
    let response = get("https://api.ipify.org")?.text()?;

    debug!("Public IP is: {}", response);

    Ok(response)
}

fn init_logger() {
    std::env::set_var("RUST_LOG", "debug");
    env_logger::init(); // Initialize the logger implementation
}

fn main() -> Result<(), Error> {
    init_logger();

    // Send a request to an external service to get the public IP
    get_router_public_ip().unwrap();

    Ok(())
}
