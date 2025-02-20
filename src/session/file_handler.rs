use log::{debug, warn};
use once_cell::sync::Lazy;
use std::error::Error;
use std::fs;
use std::fs::{read_dir, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

use chrono::{DateTime, Utc};

static ROOT_PATH: Lazy<&'static Path> = Lazy::new(|| {
    let path = Path::new(r"C:\Users\nadav\Documents\Rust\LevisDrive\DriveRoot");
    fs::create_dir_all(path).expect("Failed to create root directory");
    path
});

pub struct FilesHandler;

impl FilesHandler {
    // Create a file (or open it if it exists)
    pub fn create_file(file_path: &str) -> Result<File, Box<dyn Error>> {
        let file = File::create(ROOT_PATH.join(file_path))?;
        Ok(file)
    }

    // Open an existing file for reading
    pub fn open_file_for_reading(file_path: &str) -> Result<BufReader<File>, Box<dyn Error>> {
        let file = File::open(ROOT_PATH.join(file_path))?;
        Ok(BufReader::new(file))
    }

    // Write data to a file
    pub fn write_to_file(file_path: &str, data: &[u8]) -> Result<(), Box<dyn Error>> {
        let mut file = File::create(ROOT_PATH.join(file_path))?;
        file.write_all(data)?;
        Ok(())
    }

    // Append data to a file
    pub fn append_to_file(file_path: &str, data: &[u8]) -> Result<(), Box<dyn Error>> {
        let file = OpenOptions::new().append(true).open(file_path)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(data)?;
        Ok(())
    }

    // Read a file
    pub fn read_file(file_path: &str) -> Result<String, Box<dyn Error>> {
        let mut reader = FilesHandler::open_file_for_reading(file_path)?;
        let mut content = String::new();
        reader.read_to_string(&mut content)?;
        Ok(content)
    }

    // List all files in a directory
    pub fn list_files_in_directory(directory_path: &str) -> Result<Vec<String>, Box<dyn Error>> {
        let paths = read_dir(directory_path)?;
        let mut file_list = Vec::new();

        for path in paths {
            let path = path?.path();
            file_list.push(path.to_string_lossy().to_string());
        }

        Ok(file_list)
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

    pub fn list_dir(directory: &str) -> Result<String, Box<dyn Error>> {
        let directory_path = ROOT_PATH.join(directory);
        debug!(
            "Listing the directory: {}",
            directory_path.to_string_lossy()
        );

        let paths = fs::read_dir(directory_path).expect("Failed to read directory");

        let mut response = String::new();
        let now = Utc::now();
        let formatted_date = now.format("%b %e %H:%M").to_string();
        response.push_str("150 Opening data connection for directory list.\n");
        response.push_str(&format!(
            "drwxr-xr-x   1 admin admin        0 {} .\r\n",
            formatted_date
        ));
        response.push_str(&format!(
            "drwxr-xr-x   1 admin admin        0 {} ..\r\n",
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
        response.push_str("226 Transfer complete.\r\n");

        Ok(response)
    }

    pub fn make_directory(directory: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        fs::create_dir_all(ROOT_PATH.join(directory))?; // Creates nested directories if they don't exist

        Ok(())
    }
}
