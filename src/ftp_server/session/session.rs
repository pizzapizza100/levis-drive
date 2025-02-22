use crate::ftp_server::drive_error::DriveError;
use crate::ftp_server::posted_ip::get_router_public_ip;

use super::file_handler::FilesHandler;
use super::ftp_request::FtpRequest;
use chrono::{DateTime, Utc};
use log::{debug, warn};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::fs;
use std::io::{Read, Write};
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::thread;
use tokio::fs::File;
use tokio::io::{self, AsyncReadExt, BufReader};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

pub struct Session {
    pub is_authenticated: bool,
    pub username: Option<String>,
    pub peer_ip: String,
    pub peer_port: u16,
    available_data_channel: Option<TcpStream>,
    command_channel_lock: Arc<Mutex<TcpStream>>,
}

impl Session {
    pub async fn new(stream: TcpStream) -> Result<Self, DriveError> {
        let peer_addr = stream.peer_addr()?;

        Ok(Session {
            is_authenticated: false,
            username: None,
            peer_ip: peer_addr.ip().to_string(),
            peer_port: peer_addr.port(),
            available_data_channel: None,
            command_channel_lock: Arc::new(Mutex::new(stream)),
        })
    }

    async fn send(&self, data: &[u8]) -> Result<(), DriveError> {
        let mut stream = self.command_channel_lock.lock().await;
        stream.write_all(data).await;
        stream.flush().await?;
        Ok(())
    }

    pub async fn receive_string(&self) -> Result<String, DriveError> {
        let mut stream = self.command_channel_lock.lock().await;
        let mut reader = BufReader::new(&mut *stream);
        let mut raw_command = String::new();

        let bytes_read = reader.read_line(&mut raw_command).await?;
        if bytes_read == 0 {
            debug!("{} has disconnected.", self.peer_ip);
            return Err(DriveError::Disconnect());
        }

        stream.flush().await?;

        Ok(raw_command)
    }

    pub async fn handle_welcome(&mut self) -> Result<(), DriveError> {
        self.send(b"220 Service ready for new user.\r\n").await?;
        Ok(())
    }

    pub async fn handle_unknwon(&mut self) -> Result<(), DriveError> {
        self.send(b"500 Unknown command.\r\n").await?;
        Ok(())
    }

    pub async fn handle_user(&mut self, request: &FtpRequest) -> Result<(), DriveError> {
        // Expected format: "USER <username>"

        let username = request
            .parameters
            .get(0)
            .ok_or(DriveError::MissingParameter("missing user name parameter."))?;

        if username == "admin" || username == "anonymous" {
            self.username = Some("admin".to_string());
            debug!("Valid username, Sending \"331 User name okay, need password.\"");
            self.send(b"331 User name okay, need password.\r\n").await?;
        } else {
            debug!("Invalid username.");
            self.send(b"530 Invalid username.\r\n").await?;
        }

        Ok(())
    }

    pub async fn handle_pass(&mut self, request: &FtpRequest) -> Result<(), DriveError> {
        // Expected format: "PASS <username>"

        let password = request
            .parameters
            .get(0)
            .map(String::as_str)
            .ok_or(DriveError::MissingParameter("missing password parameter."))?;

        if self.username.is_none() {
            self.send(b"503 Bad sequence of commands. Provide USER first.\r\n")
                .await?;

            return Err(DriveError::ProtocolViolation(
                "Bad sequence of commands: Provide USER first.",
            ));
        }

        if password == "password" || password == "anonymous" {
            self.send(b"230 User logged in, proceed.\r\n").await?;
        } else {
            self.send(b"530 Invalid username.\r\n").await?;
        }

        Ok(())
    }

    pub async fn handle_type(&mut self, request: &FtpRequest) -> Result<(), DriveError> {
        // Expected format: "TYPE <transfer mode>"

        let transfer_mode =
            request
                .parameters
                .get(0)
                .map(String::as_str)
                .ok_or(DriveError::MissingParameter(
                    "missing transfer mode parameter.",
                ))?;

        match transfer_mode {
            "I" => {
                debug!("Transferring {} to binary transfer mode.", self.peer_ip);
                self.send(b"200 Switching to Binary mode.\r\n").await?;
            }
            _ => {
                self.send(b"504 Command not implemented for that parameter.\r\n")
                    .await?;
            }
        }

        Ok(())
    }

    pub async fn handle_feat(&mut self) -> Result<(), DriveError> {
        // Expected format: "FEAT"

        self.send(b"211-Extensions supported:\r\n").await?;
        self.send(b" UTF8\r\n").await?;
        self.send(b" PASV\r\n").await?;
        self.send(b"211 End\r\n").await?;
        Ok(())
        // TODO fix
    }

    pub async fn handle_opts(&mut self) -> Result<(), DriveError> {
        // Expected format: "OPTS"

        self.send(b"200 UTF8 mode enabled.\r\n").await?;
        Ok(())
    }

    pub async fn handle_syst(&mut self) -> Result<(), DriveError> {
        // Expected format: "SYST"

        self.send(b"215 215 Windows_NT.\r\n").await?;
        Ok(())
    }

    pub async fn handle_stor(&mut self, request: &FtpRequest) -> Result<(), DriveError> {
        let file_name = request
            .parameters
            .get(0)
            .map(String::as_str)
            .ok_or(DriveError::MissingParameter("missing file name parameter."))?;

        let expected_size = request
            .parameters
            .get(1)
            .ok_or(DriveError::MissingParameter("missing file size parameter."))?
            .parse::<usize>()
            .map_err(DriveError::from)?;

        let data_stream =
            self.available_data_channel
                .take()
                .ok_or(DriveError::ProtocolViolation(
                    "Requesting data request without a data channel.",
                ))?;

        let mut file = FilesHandler::create_file(file_name)?; // TODO change to open for write
        let mut reader = BufReader::new(data_stream);
        let mut buffer = vec![0; 1024];

        let mut total_bytes = 0;

        while total_bytes < expected_size {
            let bytes_read = reader.read(&mut buffer).await?; // Buffered read
            if bytes_read == 0 {
                break;
            }

            file.write_all(&buffer[..bytes_read])?;
            total_bytes += bytes_read;
        }

        reader.shutdown().await?;

        if total_bytes == expected_size {
            self.send(b"250 File uploaded successfully").await;
        } else {
            self.send(
                format!(
                    "426 Connection closed; transfer incomplete. Expected: {}, Received: {}",
                    expected_size, total_bytes,
                )
                .as_bytes(),
            )
            .await?;
        }

        Ok(())
    }

    pub async fn handle_retr(&mut self, request: &FtpRequest) -> Result<(), DriveError> {
        // Expected format: "GET <filename>"

        let file_path = request
            .parameters
            .get(0)
            .map(String::as_str)
            .ok_or(DriveError::MissingParameter("missing file path parameter."))?;

        let mut data_stream =
            self.available_data_channel
                .take()
                .ok_or(DriveError::ProtocolViolation(
                    "Requesting data request without a data channel.",
                ))?;

        let mut reader = FilesHandler::open_file_for_reading(file_path)?;

        let mut buffer = vec![0; 1024];

        while let Ok(bytes_read) = reader.read(&mut buffer) {
            if bytes_read == 0 {
                break;
            }
            data_stream.write_all(&buffer[..bytes_read]).await?;
        }

        self.send(b"250 File sent successfully\r\n").await?;
        Ok(())
    }

    pub async fn handle_list(&mut self, request: &FtpRequest) -> Result<(), DriveError> {
        // Expected format: "LIST <directory_path>"

        let directory_path =
            request
                .parameters
                .get(0)
                .map(String::as_str)
                .ok_or(DriveError::MissingParameter(
                    "missing directory path parameter.",
                ))?;

        let mut data_stream =
            self.available_data_channel
                .take()
                .ok_or(DriveError::ProtocolViolation(
                    "Requesting data request without a data channel.",
                ))?;

        let response = FilesHandler::list_dir(directory_path)?;

        debug!(
            "Sending {} directory listing of: {directory_path}",
            self.peer_ip
        );

        self.send(b"150 Opening data connection for directory list.\n")
            .await?;
        data_stream.write_all(response.as_bytes()).await?;
        self.send(b"226 Transfer complete.\r\n").await?;

        Ok(())
    }

    pub async fn handle_pasv(&mut self) -> Result<(), DriveError> {
        // Expected format: "PASV"

        if self.available_data_channel.is_some() {
            return Err(DriveError::ProtocolViolation(
                "Requesting to open new data channel when one exists.",
            ));
        }

        let start_port: u16 = 50000;
        let end_port: u16 = 60000;

        let mut rng = StdRng::from_entropy();

        let listener = loop {
            let data_port: u16 = rng.gen_range(start_port..end_port);
            debug!("Trying to bind port: {data_port}");

            match TcpListener::bind(("0.0.0.0", data_port)).await {
                Ok(listener) => break listener,
                Err(_) => continue,
            };
        };

        let local_addr = listener.local_addr()?;
        let local_port = local_addr.port();

        let router_ip = get_router_public_ip().await?;
        let segmented_ip: Vec<&str> = router_ip.split('.').collect();

        debug!(
            "Sending {} to connect on {router_ip}:{local_port}.",
            self.peer_ip
        );

        self.send(
            format!(
                "227 Entering Passive Mode ({},{},{},{},{},{}).\r\n",
                segmented_ip[0],
                segmented_ip[1],
                segmented_ip[2],
                segmented_ip[3],
                (local_port >> 8) as u8,
                (local_port & 0xFF) as u8
            )
            .as_bytes(),
        )
        .await?;

        debug!("Waiting for {} to connect on {}.", self.peer_ip, local_port);

        let (data_stream, data_addr) = listener.accept().await?;
        self.available_data_channel = Some(data_stream);

        debug!("{} connected to data socket", data_addr.ip());

        Ok(())
    }

    pub async fn handle_mkd(&mut self, request: &FtpRequest) -> Result<(), DriveError> {
        // Expected format: " MKD <directory name>"

        let directory_path =
            request
                .parameters
                .get(0)
                .map(String::as_str)
                .ok_or(DriveError::MissingParameter(
                    "missing directory path parameter.",
                ))?;

        match FilesHandler::make_directory(directory_path) {
            Ok(_) => self.send(b"257 \"{directory_name}\" created.\r\n").await?,
            Err(_) => self.send(b"550 Failed to create directory.\r\n").await?,
        }

        Ok(())
    }
    pub async fn handle_exit(&mut self) -> Result<(), DriveError> {
        // Expected format: "Quit"

        self.send(b"221 Goodbye").await?;
        Ok(())
    }
}
