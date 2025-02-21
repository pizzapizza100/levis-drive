use dotenv::dotenv;
use log::{debug, info, warn};
use std::fmt::Error;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

mod posted_ip;
mod session;

use session::session::Session;

const CHECK_POSTED_IP_INTERVAL: u64 = 60 * 5;

fn handle_client(mut stream: TcpStream) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let peer_addr = stream.peer_addr()?;
    let mut peer_session = Session::new(peer_addr.ip().to_string(), peer_addr.port(), stream);

    debug!(
        "{} has connected, sending welcome message...",
        peer_session.ip
    );
    write!(peer_session.stream, "220 Service ready for new user.\r\n")?;

    let mut reader: BufReader<TcpStream> = BufReader::new(peer_session.stream.try_clone()?);

    loop {
        let mut raw_command_line = String::new();
        debug!(
            "Waiting for new request from {}:{} on port {}",
            peer_session.ip,
            peer_session.port,
            peer_session.stream.local_addr()?.port()
        );

        let bytes_read = reader.read_line(&mut raw_command_line)?;

        if bytes_read == 0 {
            debug!("{} has disconnected.", peer_session.ip);
            break;
        }

        let command_line = raw_command_line.trim();
        debug!("{} has sent \"{}\".", peer_session.ip, command_line);

        let parts: Vec<&str> = command_line.split_whitespace().collect();

        if parts.is_empty() {
            debug!("{} Empty command", peer_session.ip);
            continue;
        }

        match parts[0].to_uppercase().as_str() {
            "GET" => {
                // Expected format: "GET <filename>"
                if parts.len() < 2 {
                    writeln!(peer_session.stream, "500 Missing filename")?;
                    continue;
                }

                let filename = parts[1];

                match peer_session.handle_get(filename) {
                    Ok(()) => debug!("Done sending {} to {}", filename, peer_session.ip),
                    Err(e) => writeln!(peer_session.stream, "550 File not found or error: {}", e)?,
                }
            }
            "USER" => {
                // Expected format: "USER <username>"
                if parts.len() < 2 {
                    writeln!(peer_session.stream, "500 Missing username")?;
                    continue;
                }

                let username = parts[1];

                match peer_session.handle_user(username) {
                    Ok(()) => (),
                    Err(e) => writeln!(peer_session.stream, "550 File not found or error: {}", e)?,
                }
            }
            "PASS" => {
                // Expected format: "PASS <username>"
                if parts.len() < 2 {
                    writeln!(peer_session.stream, "500 Missing password")?;
                    continue;
                }

                let password = parts[1];

                match peer_session.handle_pass(password) {
                    Ok(()) => debug!("{} has logged in.", peer_session.ip),
                    Err(e) => writeln!(peer_session.stream, "550 File not found or error: {}", e)?,
                }
            }
            "LIST" => {
                // Expected format: "LIST <directory_path>"
                let mut directory_path = "";
                if parts.len() == 2 {
                    directory_path = parts[1];
                }

                match peer_session.handle_list(directory_path) {
                    Ok(()) => (),
                    Err(e) => writeln!(
                        peer_session.stream,
                        "550 Failed to get directory listing: {}",
                        e
                    )?,
                }
            }
            "TYPE" => {
                // Expected format: "TYPE <transfer mode>"
                if parts.len() < 2 {
                    writeln!(peer_session.stream, "500 Missing directory")?;
                    continue;
                }

                let transfer_mode = parts[1];

                match peer_session.handle_type(transfer_mode) {
                    Ok(()) => debug!(
                        "Sending server is on binary transfer mode to {}.",
                        peer_session.ip
                    ),
                    Err(e) => writeln!(peer_session.stream, "550 File not found or error: {}", e)?,
                }
            }
            "PASV" => {
                // Expected format: "PASV"
                match peer_session.handle_pasv(raw_command_line) {
                    Ok(()) => (),
                    Err(e) => warn!("PASV command failed: {}", e),
                }
            }
            "FEAT" => {
                // Expected format: "FEAT"
                match peer_session.handle_feat() {
                    Ok(()) => debug!("Sending {} non features are implemented.", peer_session.ip),
                    Err(e) => warn!("550 File not found or error: {}", e),
                }
            }
            "OPTS" => {
                // Expected format: "OPTS"
                match peer_session.handle_opts() {
                    Ok(()) => debug!("Sending {} UTF-8 is on.", peer_session.ip),
                    Err(e) => writeln!(peer_session.stream, "550 File not found or error: {}", e)?,
                }
            }
            "SYST" => {
                // Expected format: "SYST"
                match peer_session.handle_syst() {
                    Ok(()) => debug!("Sending server is ftp to {}.", peer_session.ip),
                    Err(e) => writeln!(peer_session.stream, "550 File not found or error: {}", e)?,
                }
            }
            "MKD" => {
                // Expected format: " MKD <directory name>"
                if parts.len() < 2 {
                    writeln!(peer_session.stream, "500 Missing directory")?;
                    continue;
                }

                let directory_name = &raw_command_line[4..];
                let directory_name_cleaned = directory_name.replace("\r\n", "");

                match peer_session.handle_mkd(&directory_name_cleaned) {
                    Ok(()) => debug!("Created directory {directory_name_cleaned}."),
                    Err(e) => writeln!(peer_session.stream, "550 File not found or error: {}", e)?,
                }
            }
            "QUIT" => {
                match peer_session.handle_exit() {
                    Ok(()) => debug!("{} has logged in.", peer_session.ip),
                    Err(e) => writeln!(peer_session.stream, "550 File not found or error: {}", e)?,
                }
                debug!("{} has requested to disconnect.", peer_session.ip);
                break;
            }
            _ => {
                peer_session
                    .stream
                    .write_all("500 Unknown command".as_bytes())?;
                debug!(
                    "{} has requested unknown command. {raw_command_line}",
                    peer_session.ip
                );
            }
        }
    }

    Ok(())
}

async fn keep_posted_ip_valid() {
    loop {
        let router_public_ip = match posted_ip::get_router_public_ip() {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to get router's ip: {:?}", e);
                continue;
            }
        };

        let posted_ip = match posted_ip::get_posted_ip() {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to get posted ip: {:?}", e);
                continue;
            }
        };

        if posted_ip != router_public_ip {
            debug!(
                "Changing posted IP from: \"{}\", To \"{}\"",
                posted_ip, router_public_ip
            );

            match posted_ip::update_posted_ip(&router_public_ip) {
                Ok(_) => debug!("Posted new ip successfully"),
                Err(e) => {
                    warn!("Failed to update posted ip: {:?}", e);
                }
            }
        }

        std::thread::sleep(Duration::from_secs(CHECK_POSTED_IP_INTERVAL));
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenv().ok();
    env_logger::init(); // Initialize the logger implementation

    tokio::spawn(keep_posted_ip_valid());

    let listener = TcpListener::bind("0.0.0.0:1000").expect("Failed to bind port");
    println!("Listening on port 1000...");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(|| handle_client(stream));
            }
            Err(e) => eprintln!("Connection failed: {}", e),
        }
    }

    Ok(())
}
