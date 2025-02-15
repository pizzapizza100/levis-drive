use log::debug;
use reqwest::blocking::get;
use reqwest::Error;
use std::env;
use std::net::ToSocketAddrs;

const DUCKDNS_URL: &str = "https://www.duckdns.org/";
const LEVIS_DRIVE_DNS: &str = "levis-drive";
const DUCKDNS_TOKEN_ENV_NAME: &str = "DUCKDNS_TOKEN";

pub fn get_router_public_ip() -> Result<String, Error> {
    let response = get("https://api.ipify.org")?.text()?;
    debug!("Router's public IP is: {}", response);
    Ok(response)
}

pub fn get_posted_ip() -> Result<String, Box<dyn std::error::Error>> {
    let domain = format!("{LEVIS_DRIVE_DNS}.duckdns.org");
    let posted_ip = lookup_ip(&domain)?;
    debug!("The posted IP right now is: {}", posted_ip);
    Ok(posted_ip)
}

fn lookup_ip(domain: &str) -> Result<String, Box<dyn std::error::Error>> {
    let addr = (domain, 80).to_socket_addrs()?.next(); // Get the first resolved address

    match addr {
        Some(socket_addr) => Ok(socket_addr.ip().to_string()),
        None => Err("No address found".into()),
    }
}

fn update_posted_ip(router_public_ip: &str) -> Result<(), Error> {
    let token = env::var(format!("{DUCKDNS_TOKEN_ENV_NAME}")).expect("No Duck DNS token");

    let url = format!(
        "{DUCKDNS_URL}update?domains={LEVIS_DRIVE_DNS}&token={token}&ip={router_public_ip}"
    );

    debug!("{}", get(url)?.text()?);

    Ok(())
}

pub fn verify_posted_ip() -> Result<(), Error> {
    let router_public_ip = get_router_public_ip().unwrap();
    let posted_ip = get_posted_ip().unwrap();

    if posted_ip != router_public_ip {
        debug!(
            "Changing posted IP from: \"{}\", To \"{}\"",
            posted_ip, router_public_ip
        );
        update_posted_ip(&router_public_ip)?;
    }

    Ok(())
}
