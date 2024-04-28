mod client;
mod listener;
mod storage;
mod tracker;

use serde::{Deserialize, Serialize};

pub use client::PeerClient;
pub use listener::PeerListener;
pub use storage::MemoryStorage;

#[derive(Serialize, Deserialize, Clone)]
struct Segment {
    data: Vec<u8>,
    index: usize,
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
    SegmentNumber(usize),
}

#[derive(Serialize, Deserialize)]
enum Receive {
    FileSize(usize),
    Numbers(Option<Vec<usize>>),
    Segment(Segment),
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
