mod client;
mod listener;

use crate::client::PeerClient;
use crate::listener::PeerListener;
use serde::{Deserialize, Serialize};
use std::cmp::min;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const SEGMENT_SIZE: usize = 256 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Segment {
    index: usize,
    data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Clone)]
struct TorrentFile {
    segments: Vec<Segment>,
    name: String,
    size: usize,
}

struct TorrentClient {
    client: PeerClient,
    listener: PeerListener,
}

#[derive(Serialize, Deserialize)]
struct NamedRequest {
    name: String,
    request: OldRequest,
}

#[derive(Serialize, Deserialize)]
enum Request {
    ListenerRequest(ListenerRequest),
    ClientRequest(ClientRequest),
}

#[derive(Serialize, Deserialize)]
enum ListenerRequest {
    FetchFileInfo,
    FetchNumbers,
    FetchSegment { seg_number: usize },
    Finished,
}

#[derive(Serialize, Deserialize)]
enum ClientRequest {
    ReceiveFileInfo { size: usize },
    ReceiveNumbers { numbers: Option<Vec<usize>> },
    ReceiveSegment { segment: Segment },
    Finished,
}

#[derive(Serialize, Deserialize)]
enum OldRequest {
    FetchFileInfo,
    FetchNumbers,
    FetchSegment { seg_number: usize },
    ReceiveNumbers { numbers: Option<Vec<usize>> },
    ReceiveSegment { segment: Segment },
    ReceiveFileInfo { size: usize },
    Finished,
}

impl NamedRequest {
    fn new(name: String, request: OldRequest) -> Self {
        Self { name, request }
    }
}

impl TorrentFile {
    fn new(data: Vec<u8>, name: &str) -> Self {
        let mut index = 0;
        let mut read = 0;
        let mut segments = Vec::with_capacity(data.len() / SEGMENT_SIZE + 1);
        while read < data.len() {
            let size = min(data.len() - read, SEGMENT_SIZE);
            let segment = Segment {
                index,
                data: data[read..read + size].to_vec(),
            }; // double clone
            segments.push(segment);

            read += size;
            index += 1;
        }

        TorrentFile {
            segments,
            name: name.to_string(),
            size: data.len(),
        }
    }

    fn add_segment(&mut self, to_add: Segment) {
        if self
            .segments
            .iter()
            .find(|segment| segment.index == to_add.index)
            .is_none()
        {
            self.segments.push(to_add);
        }
    }

    fn collect_file(&mut self) -> Vec<u8> {
        let mut segments = self.segments.clone();
        segments.sort_by(|a, b| a.index.cmp(&b.index));
        segments
            .into_iter()
            .map(|segment| segment.data)
            .collect::<Vec<_>>()
            .concat()
    }

    fn is_complete(&self) -> bool {
        if self.size / SEGMENT_SIZE != self.segments.len() {
            return false;
        }

        let mut numbers = self
            .segments
            .iter()
            .map(|segment| segment.index)
            .collect::<Vec<_>>();
        numbers.sort();

        let mut current = 0;
        for number in numbers {
            if number != current {
                return false;
            }
            current += 1;
        }

        true
    }
}

mod tests {
    use crate::client::PeerClient;
    use crate::listener::PeerListener;
    use std::net::SocketAddr;
    use std::path::Path;

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn download_from_one_peer_test() {
        PeerListener::new_listen(
            Path::new("src").into(),
            SocketAddr::new("127.0.0.1".parse().unwrap(), 8001),
        );

        let mut peer1 = PeerClient::new(
            Path::new(".").into(),
            SocketAddr::new("127.0.0.1".parse().unwrap(), 8000),
        );
        peer1
            .download_file_peer(
                "file.pdf".to_string(),
                SocketAddr::new("127.0.0.1".parse().unwrap(), 8001),
            )
            .await
            .unwrap();

        let main = std::fs::read("src/file.pdf").unwrap();
        assert_eq!(
            main.len(),
            peer1
                .files
                .iter_mut()
                .find(|file| file.name == "file.pdf")
                .unwrap()
                .collect_file()
                .len()
        );
        assert_eq!(main, peer1.files[5].collect_file())
    }
}
