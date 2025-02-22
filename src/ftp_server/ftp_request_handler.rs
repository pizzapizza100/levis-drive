use crate::ftp_server::drive_error::DriveError;
use crate::ftp_server::posted_ip;
use crate::ftp_server::session::ftp_request::FtpRequest;
use crate::ftp_server::session::session::Session;
use log::{debug, info, warn};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::time::{sleep, Duration};

const CHECK_POSTED_IP_INTERVAL: u64 = 60 * 5;

pub async fn serve() {
    tokio::spawn(keep_posted_ip_valid());

    let listener = TcpListener::bind("0.0.0.0:2121")
        .await
        .expect("Failed to bind port");

    info!("Listening on port 2121...");

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(handle_client(stream));
            }
            Err(e) => warn!("Connection failed: {}", e),
        }
    }
}

async fn handle_client(stream: TcpStream) {
    let mut peer_session = match Session::new(stream).await {
        Ok(session) => session,
        Err(e) => {
            warn!("Failed to create new session: {}", e);
            return;
        }
    };

    info!(
        "{}:{} has connected, sending welcome message...",
        peer_session.peer_ip, peer_session.peer_port
    );

    if let Err(e) = peer_session.handle_welcome().await {
        warn!("Failed to create new session: {}", e);
        return;
    };

    loop {
        let request: FtpRequest = match get_new_request(&mut peer_session).await {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to receive string: {}, Exiting...", e);
                return;
            }
        };

        info!(
            "{}:{} has sent: {}.",
            peer_session.peer_ip, peer_session.peer_port, request
        );

        // Handle the request while holding the lock
        match handle_request(&mut peer_session, request).await {
            Err(DriveError::Disconnect()) => return,
            Err(e) => warn!("{}", e),
            _ => {}
        }
    }
}

async fn handle_request(peer_session: &mut Session, request: FtpRequest) -> Result<(), DriveError> {
    let result = match request.command.as_str() {
        "USER" => peer_session.handle_user(&request).await,
        "PASS" => peer_session.handle_pass(&request).await,
        "PASV" => peer_session.handle_pasv().await,
        "RETR" => {
            let session_cloned = peer_session.clone_session();
            tokio::spawn(async move {
                if let Err(e) = session_cloned.handle_retr(&request).await {
                    warn!("{}", e);
                };
            });

            Ok(())
        }
        "STOR" => {
            let session_cloned = peer_session.clone_session();
            tokio::spawn(async move {
                if let Err(e) = session_cloned.handle_stor(&request).await {
                    warn!("{}", e);
                };
            });

            Ok(())
        }
        "LIST" => {
            let session_cloned = peer_session.clone_session();
            tokio::spawn(async move {
                if let Err(e) = session_cloned.handle_list(&request).await {
                    warn!("{}", e);
                };
            });

            Ok(())
        }
        "TYPE" => peer_session.handle_type(&request).await,
        "FEAT" => peer_session.handle_feat().await,
        "OPTS" => peer_session.handle_opts().await,
        "SYST" => peer_session.handle_syst().await,
        "MKD" => peer_session.handle_mkd(&request).await,
        "PWD" => peer_session.handle_pwd().await,
        "CWD" => peer_session.handle_cwd(&request).await,
        "DELE" => peer_session.handle_dele(&request).await,
        "QUIT" => {
            info!("{} has requested to disconnect.", peer_session.peer_ip);
            peer_session.handle_exit().await?;
            Err(DriveError::Disconnect())
        }
        _ => {
            debug!("{} has requested unknown command.", peer_session.peer_ip);
            peer_session.handle_unknwon().await
        }
    };

    if result.is_ok() {
        debug!("Handled successfully");
    }
    result
}

async fn get_new_request(peer_session: &mut Session) -> Result<FtpRequest, DriveError> {
    debug!(
        "Waiting for new request from {}:{}",
        peer_session.peer_ip, peer_session.peer_port,
    );

    let received = peer_session.receive_string().await?;

    Ok(FtpRequest::new(received))
}

async fn keep_posted_ip_valid() {
    loop {
        let router_public_ip = match posted_ip::get_router_public_ip().await {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to get router's ip: {:?}", e);
                continue;
            }
        };

        let posted_ip = match posted_ip::get_posted_ip().await {
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

            match posted_ip::update_posted_ip(&router_public_ip).await {
                Ok(_) => debug!("Posted new ip successfully"),
                Err(e) => {
                    warn!("Failed to update posted ip: {:?}", e);
                }
            }
        }

        sleep(Duration::from_secs(CHECK_POSTED_IP_INTERVAL)).await;
    }
}
