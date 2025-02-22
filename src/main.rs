use dotenv::dotenv;
use std::fmt::Error;
use std::io::{BufRead, BufReader, Read, Write};
use std::thread;
use std::time::Duration;

mod ftp_server;

#[tokio::main]
async fn main() {
    dotenv().ok();
    env_logger::init();
    tokio::join!(ftp_server::serve());
}
