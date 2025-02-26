use crate::ftp_server::drive_error::DriveError;
use log::{debug, info};
use reqwest::{get, Client};
use std::{env, time::Duration};
use tokio::{net::lookup_host, time::sleep};

const LEVIS_DRIVE_DNS: &str = "levis-drive";
const DNS_USERNAME_ENV_NAME: &str = "DNS_USERNAME";
const DNS_PASSWORD_ENV_NAME: &str = "DNS_PASSWORD";
const DNS_TTL: u64 = 30;

pub async fn get_router_public_ip() -> Result<String, DriveError> {
    let response = get("https://api.ipify.org").await?;
    let response_text = response.text().await?;
    debug!("Router's public IP is: {}", response_text);
    Ok(response_text)
}

pub async fn get_posted_ip() -> Result<String, DriveError> {
    let domain = format!("{LEVIS_DRIVE_DNS}.ddnsfree.com");
    let posted_ip = lookup_ip(&domain).await?;
    debug!("The posted IP right now is: {}", posted_ip);
    Ok(posted_ip)
}

async fn lookup_ip(domain: &str) -> Result<String, DriveError> {
    let mut addr_iter = lookup_host((domain, 80)).await?;
    let addr = addr_iter
        .next()
        .ok_or_else(|| DriveError::Custom(format!("Couldn't resolve domain: {domain}")))?;

    let addr_string = addr.ip().to_string();
    debug!("Domain: \"{domain}\" resolved to: {addr_string}.");

    Ok(addr_string)
}

pub async fn update_posted_ip(router_public_ip: &str) -> Result<(), DriveError> {
    let username = env::var(DNS_USERNAME_ENV_NAME).expect("No DNS username found!");
    let password = env::var(DNS_PASSWORD_ENV_NAME).expect("No DNS password found!");

    let client = Client::new();
    let url = format!(
        "https://api.dynu.com/nic/update?hostname={}&myip={}",
        LEVIS_DRIVE_DNS, router_public_ip
    );

    let response = client
        .get(&url)
        .basic_auth(username, Some(password))
        .send()
        .await?;

    if response.status().is_success() {
        info!("Successfully updated DDNS with IP: {}", router_public_ip);
    } else {
        return Err(DriveError::Network(format!(
            "Failed to update DDNS server: {}",
            response.text().await?,
        )));
    }

    sleep(Duration::from_secs(DNS_TTL)).await;

    if get_posted_ip().await? != router_public_ip {
        return Err(DriveError::Network(
            "Failed to update server's ip".to_string(),
        ));
    }

    Ok(())
}
