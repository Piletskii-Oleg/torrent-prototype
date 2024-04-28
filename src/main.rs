use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use torrent_prototype::{MemoryStorage, PeerClient};

#[tokio::main]
async fn main() {
    let storage1 = Arc::new(Mutex::new(MemoryStorage::new(Path::new("src").into())));
    let storage2 =  Arc::new(Mutex::new(MemoryStorage::new(Path::new(".").into())));
    let _ = PeerClient::new(
        storage1,
        SocketAddr::new("127.0.0.1".parse().unwrap(), 8001),
    )
    .await;
    let mut peer1 = PeerClient::new(
        storage2,
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
