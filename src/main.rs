use dotenv::dotenv;

mod ftp_server;

#[tokio::main]
async fn main() {
    dotenv().ok();
    env_logger::init();
    tokio::join!(ftp_server::serve());
}
