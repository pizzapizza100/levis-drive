use crate::ftp_server::drive_error::DriveError;
use crate::ftp_server::posted_ip::get_router_public_ip;

use log::{debug, error, info};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::io::{ErrorKind, Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::io::BufReader;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use super::file_handler::FilesHandler;
use super::ftp_request::FtpRequest;

pub struct Session {
    pub is_authenticated: bool,
    pub username: Option<String>,
    pub peer_ip: String,
    pub peer_port: u16,
    command_channel_write: Arc<Mutex<OwnedWriteHalf>>,
    command_channel_read: Option<OwnedReadHalf>,
    data_channel: Arc<Mutex<Option<TcpStream>>>,
    cwd: PathBuf,
}

impl Session {
    pub async fn new(stream: TcpStream) -> Result<Self, DriveError> {
        let peer_addr = stream.peer_addr()?;
        let (read_half, write_half) = stream.into_split();

        Ok(Session {
            is_authenticated: false,
            username: None,
            peer_ip: peer_addr.ip().to_string(),
            peer_port: peer_addr.port(),
            command_channel_write: Arc::new(Mutex::new(write_half)),
            command_channel_read: Some(read_half),
            data_channel: Arc::new(Mutex::new(None)),
            cwd: FilesHandler::get_root_dir().to_path_buf(),
        })
    }

    pub fn clone_session(&self) -> Arc<Session> {
        Arc::new(Session {
            is_authenticated: self.is_authenticated,
            username: self.username.clone(),
            peer_ip: self.peer_ip.clone(),
            peer_port: self.peer_port,
            cwd: self.cwd.clone(),

            command_channel_write: Arc::clone(&self.command_channel_write),
            command_channel_read: None,

            data_channel: Arc::clone(&self.data_channel), // Share the same socket
        })
    }

    async fn send(&self, data: &[u8]) -> Result<(), DriveError> {
        let mut command_channel_write_locked = self.command_channel_write.lock().await;

        command_channel_write_locked.write_all(data).await?;
        command_channel_write_locked.flush().await?;
        Ok(())
    }

    pub async fn receive_string(&mut self) -> Result<String, DriveError> {
        let command_channel_read =
            self.command_channel_read
                .as_mut()
                .ok_or(DriveError::ProtocolViolation(
                    "Tried to read from command channel.",
                ))?;

        let mut reader = BufReader::new(&mut *command_channel_read);
        let mut raw_command = String::new();

        let bytes_read = reader.read_line(&mut raw_command).await?;
        if bytes_read == 0 {
            debug!("{} has disconnected.", self.peer_ip);
            return Err(DriveError::Disconnect());
        }

        Ok(raw_command)
    }

    pub async fn handle_welcome(&self) -> Result<(), DriveError> {
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
            .first()
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
            .first()
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
                .first()
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

    pub async fn handle_dele(&self, request: &FtpRequest) -> Result<(), DriveError> {
        let file_name = request
            .parameters
            .first()
            .map(String::as_str)
            .ok_or(DriveError::MissingParameter("missing file name parameter."))?;

        fs::remove_file(FilesHandler::get_root_dir().join(file_name)).await?;

        self.send(b"250 File deleted successfully.\r\n").await?;

        Ok(())
    }

    pub async fn handle_stor(&self, request: &FtpRequest) -> Result<(), DriveError> {
        let file_name = request
            .parameters
            .first()
            .map(String::as_str)
            .ok_or(DriveError::MissingParameter("missing file name parameter."))?;

        let mut data_channel_locked = self.data_channel.lock().await;

        let mut data_stream = data_channel_locked
            .take()
            .ok_or(DriveError::ProtocolViolation(
                "Requesting data request without a data channel.",
            ))?;

        let mut file = FilesHandler::create_file(file_name)?; // TODO change to open for write
        let mut buffer = vec![0; 1024 * 8];

        info!("Starting to upload: {}", file_name);

        self.send(b"150 - the server is ready and awaiting the file.\r\n")
            .await?;

        loop {
            // Wait for the socket to be readable
            data_stream.readable().await?;

            // Try to read data, this may still fail with `WouldBlock`
            // if the readiness event is a false positive.
            let bytes_read = match data_stream.try_read(&mut buffer) {
                Ok(n) => n,
                Err(ref e) if e.kind() == tokio::io::ErrorKind::WouldBlock => {
                    continue;
                }
                Err(e) => {
                    error!("Read error: {}", e);
                    return Err(DriveError::FileSystem(format!("Error reading data: {}", e)));
                }
            };

            if bytes_read == 0 {
                break;
            }

            file.write_all(&buffer[..bytes_read])?;
        }

        data_stream.shutdown().await?;

        self.send(b"226 - the file has been successfully uploaded.\r\n")
            .await?;

        info!("Done uploading: {}", file_name);

        Ok(())
    }

    pub async fn handle_retr(&self, request: &FtpRequest) -> Result<(), DriveError> {
        // Expected format: "GET <filename>"

        let file_path = match request.parameters.as_slice() {
            [] => Err(DriveError::MissingParameter("missing file path parameter.")),
            [single] => Ok(single.to_owned()),
            multiple => {
                let joined_path = multiple.join(" ");
                Ok(joined_path)
            }
        }?;

        let mut data_channel_locked = self.data_channel.lock().await;

        let mut data_stream = data_channel_locked
            .take()
            .ok_or(DriveError::ProtocolViolation(
                "Requesting data request without a data channel.",
            ))?;

        let mut reader = FilesHandler::open_file_for_reading(file_path.as_str())?;

        let mut buffer = vec![0; 1024 * 8];

        debug!("Starting to send {}'s data...", file_path);

        self.send(b"150 - File status okay; about to open data connection.\r\n")
            .await?;

        while let Ok(bytes_read) = reader.read(&mut buffer) {
            if bytes_read == 0 {
                break;
            }
            data_stream.write_all(&buffer[..bytes_read]).await?;
        }

        info!("Done sending {}'s data.", file_path);

        self.send(b"226 - Closing data connection; file transfer successful.\r\n")
            .await?;
        Ok(())
    }

    pub async fn handle_list(&self, request: &FtpRequest) -> Result<(), DriveError> {
        // Expected format: "LIST <directory_path>"

        let directory_path: &str = match request.parameters.first() {
            Some(param) => param,
            None => self.cwd.as_path().to_str().ok_or_else(|| {
                DriveError::FileSystem("Failed to convert current directory to string".into())
            })?,
        };

        let mut data_channel_locked = self.data_channel.lock().await;

        let mut data_stream = data_channel_locked
            .take()
            .ok_or(DriveError::ProtocolViolation(
                "Requesting data request without a data channel.",
            ))?;

        let response = FilesHandler::list_dir(directory_path)?;

        debug!("Sending Opening data connection message...");
        self.send(b"150 Opening data connection for directory list.\r\n")
            .await?;
        debug!("Sending data...");
        data_stream.write_all(response.as_bytes()).await?;
        debug!("Closing data socket...");
        data_stream.shutdown().await?;
        debug!("Sending Transfer complete message...");
        self.send(b"226 Transfer complete.\r\n").await?;

        Ok(())
    }

    pub async fn handle_pasv(&mut self) -> Result<(), DriveError> {
        // Expected format: "PASV"

        let mut data_channel_locked = self.data_channel.lock().await;

        if data_channel_locked.is_some() {
            return Err(DriveError::ProtocolViolation(
                "Requesting to open new data channel when one exists.",
            ));
        }

        let start_port: u16 = 50000;
        let end_port: u16 = 50100;

        let mut rng = StdRng::from_entropy();

        let listener = loop {
            let data_port: u16 = rng.gen_range(start_port..end_port);
            debug!("Trying to bind port: {data_port}");

            match TcpListener::bind(("0.0.0.0", data_port)).await {
                Ok(listener) => break listener,
                Err(e) if e.kind() == ErrorKind::AddrInUse => continue,
                Err(e) => return Err(e.into()),
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

        debug!(
            "Waiting for {} to connect on {}:{}.",
            self.peer_ip, router_ip, local_port
        );

        let (data_stream, data_addr) = listener.accept().await?;
        *data_channel_locked = Some(data_stream);

        debug!("{} connected to data socket", data_addr.ip());

        Ok(())
    }

    pub async fn handle_mkd(&mut self, request: &FtpRequest) -> Result<(), DriveError> {
        // Expected format: " MKD <directory name>"

        let directory_path = request.parameters.join(" ");

        match FilesHandler::make_directory(directory_path.as_str()) {
            Ok(_) => self.send(b"257 \"{directory_name}\" created.\r\n").await?,
            Err(_) => self.send(b"550 Failed to create directory.\r\n").await?,
        }

        Ok(())
    }

    pub async fn handle_pwd(&self) -> Result<(), DriveError> {
        // Expected format: "PWD"

        let relative_path = match self.cwd.strip_prefix(FilesHandler::get_root_dir()) {
            Ok(relative_path) => relative_path,
            Err(_) => {
                return Err(DriveError::FileSystem(format!(
                    "Failed strip root from cwd: {}",
                    self.cwd.display()
                )))
            }
        };

        let mut pwd = format!(
            "257 \"{}\" is the current directory.",
            relative_path.display()
        );

        debug!("{}'s Current working directory: {}", self.peer_ip, pwd);

        pwd.push_str("\r\n");
        self.send(pwd.as_bytes()).await?;

        Ok(())
    }

    pub async fn handle_cwd(&mut self, request: &FtpRequest) -> Result<(), DriveError> {
        let requested_path = match request.parameters.as_slice() {
            [] => FilesHandler::get_root_dir().to_path_buf(),
            [s] if s == "\\" => FilesHandler::get_root_dir().to_path_buf(),
            [single] => FilesHandler::get_root_dir().join(single),
            _ => FilesHandler::get_root_dir().join(request.parameters.join(" ")),
        };

        debug!("Requested directory: {}", requested_path.display());

        if !requested_path.exists() {
            self.send(
                format!(
                    "550 {}: No such file or directory.\r\n",
                    requested_path.to_string_lossy()
                )
                .as_bytes(),
            )
            .await?;
            return Err(DriveError::FileSystem(format!(
                "No such file or directory: {}",
                requested_path.to_string_lossy()
            )));
        }

        self.cwd = requested_path;

        debug!(
            "Changing {}'s Current Working Directory to: {}",
            self.peer_ip,
            self.cwd.to_string_lossy()
        );

        self.send(b"250 Directory successfully changed.\r\n")
            .await?;

        Ok(())
    }

    pub async fn handle_exit(&self) -> Result<(), DriveError> {
        // Expected format: "Quit"

        self.send(b"221 Goodbye").await?;
        Ok(())
    }
}
