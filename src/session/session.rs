use super::file_handler::FilesHandler;
use chrono::{DateTime, Utc};
use log::{debug, warn};
use rand::Rng;
use std::fs;
use std::io;
use std::io::{BufReader, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::process::Command;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

pub struct Session {
    pub is_authenticated: bool,
    pub username: Option<String>,
    pub ip: String,
    pub stream: TcpStream,
    pub port: u16,
}

impl Session {
    pub fn new(ip: String, port: u16, stream: TcpStream) -> Self {
        Session {
            is_authenticated: false,
            username: None,
            ip,
            stream,
            port,
        }
    }

    pub fn handle_user(&mut self, username: &str) -> Result<(), Box<dyn std::error::Error>> {
        if username == "admin" || username == "anonymous" {
            self.username = Some("admin".to_string());
            debug!("Valid username, Sending \"331 User name okay, need password.\"");
            write!(self.stream, "331 User name okay, need password.\r\n")?;
        } else {
            debug!("Invalid username.");
            self.stream
                .write_all("530 Invalid username.\r\n".as_bytes())?;
        }
        self.stream.flush()?;
        Ok(())
    }

    pub fn handle_pass(&mut self, password: &str) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(username) = &self.username {
            if password == "password" || password == "anonymous" {
                write!(self.stream, "230 User logged in, proceed.\r\n")?;
            } else {
                self.stream.write_all(
                    "503 Bad sequence of commands. Provide USER first.\r\n".as_bytes(),
                )?;
            }
        } else {
            self.stream
                .write_all("530 Invalid username.\r\n".as_bytes())?;
        }

        Ok(())
    }

    pub fn handle_type(&mut self, transfer_mode: &str) -> Result<(), Box<dyn std::error::Error>> {
        match transfer_mode {
            "I" => {
                debug!("Transferring {} to binary transfer mode.", self.ip);
                self.stream
                    .write_all(b"200 Switching to Binary mode.\r\n")?;
            }
            _ => {
                self.stream
                    .write_all(b"504 Command not implemented for that parameter.\r\n")?;
            }
        }

        self.stream.flush()?;
        Ok(())
    }

    pub fn handle_feat(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.stream.write_all(b"211-Extensions supported:\r\n")?;
        self.stream.write_all(b" UTF8\r\n")?;
        self.stream.write_all(b" PASV\r\n")?;
        self.stream.write_all(b"211 End\r\n")?;
        self.stream.flush()?;
        Ok(())
    }

    pub fn handle_opts(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.stream.write_all(b"200 UTF8 mode enabled.\r\n")?;
        self.stream.flush()?;
        Ok(())
    }

    pub fn handle_syst(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.stream.write_all(b"215 215 Windows_NT.\r\n")?;
        self.stream.flush()?;
        Ok(())
    }

    pub fn handle_put(
        stream: &mut TcpStream,
        filename: &str,
        file_size: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut file = FilesHandler::create_file(filename)?; // Create or overwrite the file
        let mut buffer = vec![0; 1024];

        let bytes_written = loop {
            let bytes_read = stream.read(&mut buffer)?;
            if bytes_read == 0 {
                break file.metadata()?.len(); // Exit the loop, return file size
            }
            file.write_all(&buffer[..bytes_read])?;
        };

        if file_size == bytes_written as usize {
            stream.write_all("250 File uploaded successfully".as_bytes())?;
        } else if file_size > bytes_written as usize {
            stream.write_all("426 Connection closed; transfer incomplete.".as_bytes())?;
        } else {
            // file_size < bytes_written as usize
            stream.write_all("553 File size mismatch.".as_bytes())?;
        }

        Ok(())
    }

    pub fn handle_get(&mut self, filename: &str) -> Result<(), Box<dyn std::error::Error>> {
        let file = FilesHandler::open_file_for_reading(filename)?;
        let mut reader = BufReader::new(file);

        let mut buffer = vec![0; 1024];

        while let Ok(bytes_read) = reader.read(&mut buffer) {
            if bytes_read == 0 {
                break; // EOF
            }
            self.stream.write_all(&buffer[..bytes_read])?;
        }

        self.stream
            .write_all("250 File sent successfully".as_bytes())?;
        Ok(())
    }

    pub fn handle_list(&mut self, directory: &str) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Listing the directory: {directory}");
        let response = FilesHandler::list_dir(directory)?;
        debug!("Directory listing of {directory}: \n{response}");
        self.stream.write_all(response.as_bytes())?;

        Ok(())
    }

    pub fn handle_pasv(
        &mut self,
        command: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let start_port: u16 = 50000;
        let end_port: u16 = 50010;

        let mut rng = rand::thread_rng();

        let listener = loop {
            let data_port: u16 = rng.gen_range(start_port..end_port);
            debug!("Trying to bind port: {data_port}");
            match TcpListener::bind(("192.168.1.100", data_port)) {
                Ok(listener) => break listener,
                Err(e) => continue,
            };
        };

        let local_addr = listener.local_addr()?;
        let port = local_addr.port();
        let ip = local_addr.ip();

        debug!("Sending {} to connect on {ip}:{port}.", self.ip);

        // Calculate high and low bytes of the port.
        let p1 = (port >> 8) as u8;
        let p2 = (port & 0xFF) as u8;

        let response = match ip {
            IpAddr::V4(ipv4) => {
                format!(
                    "227 Entering Passive Mode (192,168,1,100,{},{}).\r\n",
                    p1, p2
                )
            }
            _ => String::from("Not an IPv4 address..."),
        };

        self.stream.write_all(response.as_bytes())?;

        debug!("Waiting for {} to connect on {port}.", self.ip);

        let (tx_filename, rx_filename) = mpsc::channel();
        let (tx_done, rx_done) = mpsc::channel();

        self.stream.write_all(b"150 Opening data connection.\r\n")?;

        let handle = Session::handle_data_connection(listener, rx_filename, tx_done);

        // Send file request to Data Handler
        tx_filename.send(command).expect("Failed to send filename");

        // Wait for Data Handler to complete transfer
        rx_done
            .recv()
            .expect("Failed to receive transfer completion");

        self.stream.write_all(b"226 Transfer complete.\r\n")?;

        handle.join(); // TODO add match

        Ok(())
    }

    pub fn handle_mkd(
        &mut self,
        directory_name: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match FilesHandler::make_directory(directory_name) {
            Ok(_) => write!(self.stream, "257 \"{directory_name}\" created.\r\n")?,
            Err(_) => self
                .stream
                .write_all(b"550 Failed to create directory.\r\n")?,
        }

        Ok(())
    }
    pub fn handle_exit(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.stream.write_all("221 Goodbye".as_bytes())?;
        Ok(())
    }

    pub fn handle_data_connection(
        listener: TcpListener,
        rx: Receiver<String>,
        tx: Sender<()>,
    ) -> thread::JoinHandle<()> {
        let data_port = listener.local_addr().unwrap().port();
        debug!("[Data socket] Opened a new socket on port: {}", data_port);

        thread::spawn(move || {
            let (mut data_stream, _) = listener.accept().expect("Failed to accept data connection");
            debug!(
                "[Data socket] {} connected to data socket",
                data_stream.peer_addr().unwrap().ip()
            );
            let request = rx.recv().expect("Failed to receive filename");

            let parts: Vec<&str> = request.split_whitespace().collect();

            match &request {
                request if request.starts_with("PUT") => {
                    debug!(
                        "Getting {} file \"{}\", Size: {}",
                        data_stream.peer_addr().unwrap().ip(),
                        parts[0],
                        parts[1]
                    );
                    Session::handle_put(
                        &mut data_stream,
                        parts[0],
                        parts[1].parse::<usize>().expect("Can't parse file size."),
                    )
                    .unwrap();
                    debug!("Done getting file.");
                }
                request => {
                    // client testing?
                    let cleaned_request = &request.replace("\r\n", "");
                    debug!("[Data socket] got command \"{cleaned_request}\".");
                }
            }

            // Signal command thread that transfer is done
            tx.send(()).expect("Failed to signal completion");

            debug!("[Data socket] Closing data socket on port {}.", data_port);
            data_stream
                .try_clone()
                .expect("Failed to close data socket");
        })
    }
}
