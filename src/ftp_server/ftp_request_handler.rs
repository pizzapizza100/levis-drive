use std::sync::Arc;

use crate::ftp_server::drive_error::DriveError;
use crate::ftp_server::posted_ip;
use crate::ftp_server::session::file_handler::FilesHandler;
use crate::ftp_server::session::ftp_request::FtpRequest;

use crate::ftp_server::session::session::Session;
use log::trace;
use log::{debug, error, info, warn};
use rustls::{Certificate, PrivateKey, ServerConfig};
use std::fs::File;
use std::io::BufReader;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::time::{sleep, Duration};
use tokio_postgres::types::private::BytesMut;
use tokio_rustls::TlsAcceptor;

const CHECK_POSTED_IP_INTERVAL: u64 = 60 * 5;

pub type DriveResult<T> = Result<T, DriveError>;

pub async fn serve() {
    tokio::spawn(keep_posted_ip_valid());

    let certificate = match load_certificates("cert.pem") {
        Ok(certificate) => certificate,
        Err(_) => {
            error!("No TLS certificate found!");
            return;
        }
    };

    let key = match load_private_key("key.pem") {
        Ok(key) => key,
        Err(_) => {
            error!("No TLS key file found!");
            return;
        }
    };

    let tls_config = match ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(certificate, key)
    {
        Ok(tls_config) => tls_config,
        Err(_) => {
            error!("Invalid certificate or key!");
            return;
        }
    };

    let acceptor = Arc::from(TlsAcceptor::from(Arc::from(tls_config)));

    if FilesHandler::init_root_path().await.is_err() {
        error!("Failed to init the root folder!");
        return;
    }

    let listener = TcpListener::bind("0.0.0.0:2121")
        .await
        .expect("Failed to bind port");

    info!("Listening on port 2121...");

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(handle_client(acceptor.clone(), stream));
            }
            Err(e) => warn!("Connection failed: {}", e),
        }
    }
}

async fn handle_client(acceptor: Arc<TlsAcceptor>, mut stream: TcpStream) {
    info!(
        "{} connected, Starting TLS handshake...",
        stream.peer_addr().unwrap().ip()
    );

    stream
        .write_all(b"220 Service ready for new user.\r\n")
        .await
        .unwrap();

    let mut buffer = BytesMut::with_capacity(10);
    let size = stream.read_buf(&mut buffer).await.unwrap();

    debug!("request: {},{:?}", size, buffer);
    stream
        .write_all(b"234 Proceed with TLS handshake\r\n")
        .await
        .unwrap();

    let tls_stream = match acceptor.accept(stream).await {
        Ok(s) => s,
        Err(e) => {
            trace!("Full TLS handshake error: {:?}", e);

            warn!("TLS handshake failed: {:?}", e);
            return;
        }
    };

    let mut peer_session = match Session::new(acceptor, tls_stream).await {
        Ok(session) => session,
        Err(e) => {
            warn!("Failed to create new session: {}", e);
            return;
        }
    };

    if peer_session.is_lan_connection {
        info!(
            "{}:{} has connected locally, sending welcome message...",
            peer_session.peer_ip, peer_session.peer_port
        );
    } else {
        info!(
            "{}:{} has connected, sending welcome message...",
            peer_session.peer_ip, peer_session.peer_port
        );
    }

    if let Err(e) = peer_session.handle_welcome().await {
        warn!("Failed to create new session: {}", e);
        return;
    };

    loop {
        let request: FtpRequest = match get_new_request(&mut peer_session).await {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    "{}:{} failed to receive string: {}, Disconnecting...",
                    peer_session.peer_ip, peer_session.peer_port, e
                );

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
            Err(e) => {
                warn!("{}", e);

                if let Err(send_result) = peer_session.send(e.to_string().as_bytes()).await {
                    warn!("{}", send_result);
                };
            }
            _ => {}
        }
    }
}

async fn handle_request(peer_session: &mut Session, request: FtpRequest) -> Result<(), DriveError> {
    let result = match request.command.as_str() {
        "USER" => peer_session.handle_user(&request).await,
        "PASS" => peer_session.handle_pass(&request).await,
        "PROT" => peer_session.handle_prot(&request).await,
        "PBSZ" => peer_session.handle_pbsz(&request).await,
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
        "RNFR" => peer_session.handle_rnfr(&request).await,
        "RNTO" => peer_session.handle_rnto(&request).await,
        "MKD" => peer_session.handle_mkd(&request).await,
        "RMD" => peer_session.handle_rmd(&request).await,
        "PWD" => peer_session.handle_pwd().await,
        "CWD" => peer_session.handle_cwd(&request).await,
        "DELE" => peer_session.handle_dele(&request).await,
        "NOOP" => peer_session.handle_noop().await,
        "QUIT" => {
            info!("{} has requested to disconnect.", peer_session.peer_ip);
            peer_session.handle_quit().await?;
            Err(DriveError::Disconnect())
        }
        _ => {
            warn!("{} has requested unknown command.", peer_session.peer_ip);
            peer_session.handle_unknown().await
        }
    };

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

fn load_certificates(path: &str) -> DriveResult<Vec<Certificate>> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    // Collect the iterator into a Result<Vec<CertificateDer>, std::io::Error>
    let certs_der = rustls_pemfile::certs(&mut reader).collect::<Result<Vec<_>, _>>()?;

    // Map each CertificateDer to a rustls::Certificate
    let certs = certs_der
        .into_iter()
        .map(|cert_der| Certificate(cert_der.as_ref().to_vec())) // Use as_ref() to access the inner Vec<u8>
        .collect();

    Ok(certs)
}

fn load_private_key(path: &str) -> DriveResult<PrivateKey> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    // Read the first key from the file
    let private_key = rustls_pemfile::pkcs8_private_keys(&mut reader)
        .next()
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "No private key found")
        })??;

    Ok(PrivateKey(private_key.secret_pkcs8_der().to_vec()))
}

async fn keep_posted_ip_valid() {
    let mut failed_request_count = 0;

    loop {
        if failed_request_count == 3 {
            panic!("No internet!");
        }

        let router_public_ip = match posted_ip::get_router_public_ip().await {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to get router's ip: {:?}", e);
                failed_request_count += 1;
                continue;
            }
        };

        let posted_ip = match posted_ip::get_posted_ip().await {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to get posted ip: {:?}", e);
                failed_request_count += 1;
                continue;
            }
        };

        if posted_ip != router_public_ip {
            info!(
                "Changing posted IP from: \"{}\", To \"{}\"",
                posted_ip, router_public_ip
            );

            match posted_ip::update_posted_ip(&router_public_ip).await {
                Ok(_) => debug!("Posted new ip successfully"),
                Err(e) => {
                    warn!("Failed to update posted ip: {:?}", e);
                }
            }
        } else {
            debug!("No need to update IP.");
        }

        sleep(Duration::from_secs(CHECK_POSTED_IP_INTERVAL)).await;
    }
}
