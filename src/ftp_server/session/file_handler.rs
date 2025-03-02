use chrono::{DateTime, Local};
use log::{debug, warn};
use std::io;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::BufReader;
use tokio::sync::OnceCell;

use crate::ftp_server::drive_error::DriveError;

static ROOT_PATH: OnceCell<PathBuf> = OnceCell::const_new();

pub type DriveResult<T> = Result<T, DriveError>;

pub struct FilesHandler;

// TODO change to async
impl FilesHandler {
    /// Initializes the global root directory asynchronously.
    /// This must be called once (e.g. at application startup) before any file operations.
    pub async fn init_root_path() -> DriveResult<()> {
        let root = PathBuf::from(r"D:\LevisDriveRoot");
        fs::create_dir_all(&root).await?;
        // If already set, return an error; otherwise, set the root path.
        ROOT_PATH
            .set(root)
            .map_err(|_| DriveError::Custom("Initiating root dir when initiated.".to_string()))?;
        Ok(())
    }

    /// Creates or truncates a file asynchronously.
    /// Opens a file for writing; if the file doesn't exist, it is created.
    pub async fn create_file(file_path: &impl AsRef<Path>) -> DriveResult<fs::File> {
        let root = ROOT_PATH.get().expect("Root path not initialized");
        let full_path = root.join(file_path);
        // You can also use OpenOptions here if different behaviors are needed.
        let file = fs::File::create(full_path).await?;
        Ok(file)
    }

    /// Opens an existing file for reading asynchronously.
    pub async fn open_file_for_reading(
        file_path: &impl AsRef<Path>,
    ) -> DriveResult<BufReader<fs::File>> {
        let root = ROOT_PATH.get().expect("Root path not initialized");
        let full_path = root.join(file_path);
        let file = fs::File::open(full_path).await?;
        Ok(BufReader::new(file))
    }

    /// Checks asynchronously if a file or directory exists.
    pub async fn check_exists(file_path: &impl AsRef<Path>) -> DriveResult<bool> {
        let root = ROOT_PATH.get().expect("Root path not initialized");
        let full_path = root.join(file_path);
        match fs::metadata(&full_path).await {
            Ok(_) => Ok(true),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// Renames a file or directory asynchronously.
    pub async fn rename(
        old_file: &impl AsRef<Path>,
        new_file_path: &impl AsRef<Path>,
    ) -> DriveResult<()> {
        let root = ROOT_PATH.get().expect("Root path not initialized");
        let old_full = root.join(old_file);
        let new_full = root.join(new_file_path);
        fs::rename(old_full, new_full).await?;
        Ok(())
    }

    /// Generates a Unix-like permission string from file metadata.
    fn get_unix_permissions(metadata: &std::fs::Metadata) -> String {
        // Pre-allocate a string with capacity for efficiency.
        let mut perms = String::with_capacity(10);
        // First character: directory ('d') or file ('-')
        perms.push(if metadata.is_dir() { 'd' } else { '-' });
        // Permissions based on read-only flag
        let readonly = metadata.permissions().readonly();
        if readonly {
            perms.push_str("r--r--r--");
        } else {
            perms.push_str("rw-rw-rw-");
        }
        perms
    }

    /// Lists the contents of a directory asynchronously in a Unix 'ls -l' style format.
    pub async fn list_dir(directory: &impl AsRef<Path>) -> DriveResult<String> {
        let root = ROOT_PATH.get().expect("Root path not initialized");
        let directory_path = root.join(directory);

        debug!("Listing directory: {}", directory_path.display());

        let mut read_dir = fs::read_dir(&directory_path).await?;
        let mut response = String::new();
        let now: DateTime<Local> = Local::now();
        let formatted_date = now.format("%b %e %H:%M").to_string();

        // Hard-code the entries for '.' and '..'
        response.push_str(&format!(
            "drwxr-xr-x 1 admin admin  0 {} .\r\n",
            formatted_date
        ));
        response.push_str(&format!(
            "drwxr-xr-x 1 admin admin  0 {} ..\r\n",
            formatted_date
        ));

        while let Some(entry) = read_dir.next_entry().await? {
            let full_path = entry.path();
            let path = full_path.strip_prefix(root)?;

            let metadata = entry.metadata().await?;
            let file_size = metadata.len();

            let modified_time = metadata.modified()?;
            let modified_datetime: DateTime<Local> = modified_time.into();
            let modified_time_formatted = modified_datetime.format("%b %d %H:%M").to_string();

            let permissions = FilesHandler::get_unix_permissions(&metadata);
            let file_name = match path.file_name() {
                Some(os_str) => os_str.to_string_lossy(),
                None => {
                    warn!("Failed to get file name for an entry, skipping.");
                    continue;
                }
            };

            // Use formatted alignment for the file size.
            response.push_str(&format!(
                "{} 1 admin admin {:>8} {} {}\r\n",
                permissions, file_size, modified_time_formatted, file_name
            ));
        }

        Ok(response)
    }

    /// Creates a directory and its parent components asynchronously.
    pub async fn make_directory(directory: &impl AsRef<Path>) -> DriveResult<()> {
        let root = ROOT_PATH.get().expect("Root path not initialized");
        let dir_path = root.join(directory);
        fs::create_dir_all(dir_path).await?;
        Ok(())
    }

    /// Removes a directory and its contents asynchronously.
    pub async fn remove_directory(directory: &impl AsRef<Path>) -> DriveResult<()> {
        let root = ROOT_PATH.get().expect("Root path not initialized");
        let dir_path = root.join(directory);
        fs::remove_dir_all(dir_path).await?;
        Ok(())
    }

    /// Returns a reference to the root directory.
    pub fn get_root_dir() -> &'static Path {
        ROOT_PATH
            .get()
            .expect("Root path not initialized")
            .as_path()
    }
}
