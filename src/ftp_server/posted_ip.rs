use crate::ftp_server::drive_error::DriveError;
use log::debug;
use reqwest::get;
use std::env;
use tokio::net::lookup_host;

const DUCK_DNS_URL: &str = "https://www.duckdns.org/";
const LEVIS_DRIVE_DNS: &str = "levis-drive";
const DUCK_DNS_TOKEN_ENV_NAME: &str = "DUCK_DNS_TOKEN";

pub async fn get_router_public_ip() -> Result<String, DriveError> {
    let response = get("https://api.ipify.org").await?;
    let response_text = response.text().await?;
    debug!("Router's public IP is: {}", response_text);
    Ok(response_text)
}

pub async fn get_posted_ip() -> Result<String, DriveError> {
    let domain = format!("{LEVIS_DRIVE_DNS}.duckdns.org");
    let posted_ip = lookup_ip(&domain).await?;
    debug!("The posted IP right now is: {}", posted_ip);
    Ok(posted_ip)
}

async fn lookup_ip(domain: &str) -> Result<String, DriveError> {
    let mut addr_iter = lookup_host((domain, 80)).await?;
    let addr = addr_iter
        .next()
        .ok_or_else(|| DriveError::Custom(format!("Couldn't resolve domain: {domain}")))?;

    Ok(addr.ip().to_string())
}

pub async fn update_posted_ip(router_public_ip: &str) -> Result<(), DriveError> {
    let token = env::var(DUCK_DNS_TOKEN_ENV_NAME).expect("No Duck DNS token");

    let url = format!(
        "{DUCK_DNS_URL}update?domains={LEVIS_DRIVE_DNS}&token={token}&ip={router_public_ip}"
    );

    debug!("Server response: {}", get(url).await?.text().await?);

    Ok(())
}
