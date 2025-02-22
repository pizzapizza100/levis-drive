mod drive_error;
mod ftp_request_handler;
mod posted_ip;
mod session;

pub async fn serve() {
    // data base init?
    ftp_request_handler::serve().await;
}
