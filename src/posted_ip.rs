use log::debug;
use reqwest::blocking::get;
use reqwest::Error;
use std::env;
use std::net::ToSocketAddrs;

const DUCK_DNS_URL: &str = "https://www.duckdns.org/";
const LEVIS_DRIVE_DNS: &str = "levis-drive";
const DUCK_DNS_TOKEN_ENV_NAME: &str = "DUCK_DNS_TOKEN";

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
    let mut addr_iter = (domain, 80).to_socket_addrs()?;
    let addr = addr_iter
        .next()
        .ok_or_else(|| "No address found.".to_string())?;

    Ok(addr.ip().to_string())
}

pub fn update_posted_ip(router_public_ip: &str) -> Result<(), Error> {
    let token = env::var(DUCK_DNS_TOKEN_ENV_NAME).expect("No Duck DNS token");

    let url = format!(
        "{DUCK_DNS_URL}update?domains={LEVIS_DRIVE_DNS}&token={token}&ip={router_public_ip}"
    );

    debug!("Server response: {}", get(url)?.text()?);

    Ok(())
}
