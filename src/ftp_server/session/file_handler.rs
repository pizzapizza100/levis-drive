use crate::ftp_server::drive_error::DriveError;
use log::{debug, warn};
use once_cell::sync::Lazy;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use chrono::{DateTime, Utc};

static ROOT_PATH: Lazy<&'static Path> = Lazy::new(|| {
    let path = Path::new(r"C:\Users\nadav\Documents\Rust\LevisDrive\DriveRoot");
    fs::create_dir_all(path).expect("Failed to create root directory");
    path
}); // TODO fix this

pub struct FilesHandler;

// TODO change to async
impl FilesHandler {
    // Create a file (or open it if it exists)
    pub fn create_file(file_path: &str) -> Result<File, DriveError> {
        let file = File::create(ROOT_PATH.join(file_path))?;
        Ok(file)
    }

    // Open an existing file for reading
    pub fn open_file_for_reading(file_path: &str) -> Result<BufReader<File>, DriveError> {
        let file = File::open(ROOT_PATH.join(file_path))?;
        Ok(BufReader::new(file))
    }

    fn get_unix_permissions(metadata: &fs::Metadata) -> String {
        let mut perms = String::new();

        // First character: directory or file
        if metadata.is_dir() {
            perms.push('d');
        } else {
            perms.push('-');
        }

        // Use the read-only flag to decide permissions
        let readonly = metadata.permissions().readonly();
        if readonly {
            perms.push_str("r--r--r--");
        } else {
            perms.push_str("rw-rw-rw-");
        }

        perms
    }

    pub fn list_dir(directory: &str) -> Result<String, DriveError> {
        let directory_path = ROOT_PATH.join(directory);
        debug!(
            "Listing the directory: {}",
            directory_path.to_string_lossy()
        );

        let paths = fs::read_dir(directory_path).expect("Failed to read directory");

        let mut response = String::new();
        let now = Utc::now();
        let formatted_date = now.format("%b %e %H:%M").to_string();
        response.push_str(&format!(
            "drwxr-xr-x 1 admin admin  0 {} .\r\n",
            formatted_date
        ));
        response.push_str(&format!(
            "drwxr-xr-x 1 admin admin 0 {} ..\r\n",
            formatted_date
        ));

        for entry in paths {
            match entry {
                Ok(entry) => {
                    let path = entry.path();
                    let metadata = entry.metadata()?;
                    let file_size = metadata.len();
                    let modified_time = metadata.modified()?;
                    let modified_datetime: DateTime<Utc> = modified_time.into();
                    let modified_time_formatted =
                        modified_datetime.format("%b %d %H:%M").to_string();
                    let permissions = FilesHandler::get_unix_permissions(&metadata);
                    let file_name = match path.file_name() {
                        Some(os_str) => os_str.to_string_lossy(),
                        None => {
                            warn!("Failed to read file name the entry... continuing to the next one...");
                            continue;
                        }
                    };

                    response.push_str(&format!(
                        "{} 1 admin admin {} {} {}\r\n",
                        permissions, file_size, modified_time_formatted, file_name
                    ));
                }
                Err(e) => warn!("Failed to read entry: {}", e),
            }
        }

        Ok(response)
    }

    pub fn make_directory(directory: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        fs::create_dir_all(ROOT_PATH.join(directory))?;
        Ok(())
    }

    pub fn get_root_dir() -> &'static Path {
        ROOT_PATH.as_ref()
    }
}
