// Data class
use std::fmt;

pub struct FtpRequest {
    pub command: String,
    pub parameters: Vec<String>,
}

impl FtpRequest {
    pub fn new(full_command: String) -> Self {
        let parts: Vec<&str> = full_command.split_whitespace().collect();

        FtpRequest {
            command: parts[0].to_string(),
            parameters: parts[1..].iter().map(|&s| s.to_string()).collect(),
        }
    }
}

impl fmt::Display for FtpRequest {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if !self.parameters.is_empty() {
            write!(
                f,
                "Command: \"{}\", parameters: [{}]",
                self.command,
                self.parameters.join(", ")
            )
        } else {
            write!(f, "Command: \"{}\"", self.command)
        }
    }
}
