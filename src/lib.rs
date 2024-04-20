use std::net::SocketAddr;
use std::path::Path;
use tokio::fs::File;

const KB: usize = 1024;

struct Segment {
    number: usize,
    data: [u8; 256 * KB]
}

struct Peer {
    files: Vec<File>,
    // peers: Vec<SocketAddr> - should be session-wide only
}

impl Peer {
    pub async fn new(folder: &Path) -> Self {
        Peer { files: vec![] } // read files from folder
    }

    async fn fetch_peers(tracker: SocketAddr) -> Vec<SocketAddr> {
        unimplemented!()
    }
}
