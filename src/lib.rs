mod client;
mod listener;
mod tracker;

use serde::{Deserialize, Serialize};
use std::cmp::min;

pub use client::PeerClient;
pub use listener::PeerListener;

const SEGMENT_SIZE: usize = 256 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Segment {
    index: usize,
    data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TorrentFile {
    segments: Vec<Segment>,
    pub name: String,
    pub size: usize,
}

#[derive(Serialize, Deserialize)]
struct NamedRequest {
    name: String,
    request: Request,
}

#[derive(Serialize, Deserialize)]
enum Request {
    Listener(Fetch),
    Client(Receive),
    Finished,
}

#[derive(Serialize, Deserialize)]
enum Fetch {
    FileInfo,
    Numbers,
    Segment { seg_number: usize },
}

#[derive(Serialize, Deserialize)]
enum Receive {
    FileInfo { size: usize },
    Numbers { numbers: Option<Vec<usize>> },
    Segment { segment: Segment },
}

impl NamedRequest {
    fn new(name: String, request: Request) -> Self {
        Self { name, request }
    }
}

impl From<Fetch> for Request {
    fn from(value: Fetch) -> Self {
        Self::Listener(value)
    }
}

impl From<Receive> for Request {
    fn from(value: Receive) -> Self {
        Self::Client(value)
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
        if !self
            .segments
            .iter()
            .any(|segment| segment.index == to_add.index)
        {
            self.segments.push(to_add);
        }
    }

    pub fn collect_file(&mut self) -> Vec<u8> {
        let mut segments = self.segments.clone();
        segments.sort_by(|a, b| a.index.cmp(&b.index));
        segments
            .into_iter()
            .map(|segment| segment.data)
            .collect::<Vec<_>>()
            .concat()
    }

    fn is_complete(&self) -> bool {
        if self.size
            != self
                .segments
                .iter()
                .map(|seg| seg.data.len())
                .sum::<usize>()
        {
            return false;
        }

        let mut numbers = self
            .segments
            .iter()
            .map(|segment| segment.index)
            .collect::<Vec<_>>();
        numbers.sort();

        for (current, number) in numbers.into_iter().enumerate() {
            if number != current {
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use crate::client::PeerClient;
    use std::net::SocketAddr;
    use std::path::Path;

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn download_from_one_peer_test() {
        let mut peer0 = PeerClient::new(
            Path::new("src").into(),
            SocketAddr::new("127.0.0.1".parse().unwrap(), 8000),
            SocketAddr::new("127.0.0.1".parse().unwrap(), 8001),
        )
        .await;
        let mut peer1 = PeerClient::new(
            Path::new(".").into(),
            SocketAddr::new("127.0.0.1".parse().unwrap(), 8000),
            SocketAddr::new("127.0.0.1".parse().unwrap(), 9000),
        )
        .await;
        peer1
            .download_file_peer(
                "file.pdf".to_string(),
                SocketAddr::new("127.0.0.1".parse().unwrap(), 8001),
            )
            .await
            .unwrap();

        let main = std::fs::read("src/file.pdf").unwrap();
        let mut vec_guard = peer1.files.lock().unwrap();
        let file = vec_guard
            .iter_mut()
            .find(|file| file.name == "file.pdf")
            .unwrap()
            .collect_file();
        assert_eq!(main.len(), file.len());
        assert_eq!(main, file)
    }
}
