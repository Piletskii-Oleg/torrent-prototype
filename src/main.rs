use std::net::SocketAddr;
use std::path::Path;
use torrent_prototype::PeerClient;

#[tokio::main]
async fn main() {
    let _ = PeerClient::new(
        Path::new("src").into(),
        SocketAddr::new("127.0.0.1".parse().unwrap(), 8001),
    )
    .await;
    let mut peer1 = PeerClient::new(
        Path::new(".").into(),
        SocketAddr::new("127.0.0.1".parse().unwrap(), 9000),
    )
    .await;

    let handles = peer1.download_file("file.pdf".to_string()).await.unwrap();
    for handle in handles {
        handle.await.unwrap();
    }
    //let main = std::fs::read("src/file.pdf").unwrap();
    // assert_eq!(
    //     main.len(),
    //     peer1.files[5].size
    // );
    //assert_eq!(main, peer1.files[5].collect_file(), "contents did not match")
}
