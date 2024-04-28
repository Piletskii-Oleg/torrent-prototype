mod client;
mod listener;
mod tracker;

use serde::{Deserialize, Serialize};
use std::cmp::min;

pub use client::PeerClient;
pub use listener::PeerListener;

const KB: usize = 1024;

const SEGMENT_SIZE: usize = 256 * KB;

#[derive(Serialize, Deserialize, Clone)]
struct Segment {
    index: usize,
    data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TorrentFile {
    segments: Vec<Segment>,
    name: String,
    intended_size: usize,
    actual_size: usize,
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
            intended_size: data.len(),
            actual_size: data.len(),
        }
    }

    fn new_empty(name: &str, size: usize) -> Self {
        Self {
            segments: vec![],
            name: name.to_string(),
            intended_size: size,
            actual_size: 0,
        }
    }

    fn add_segment(&mut self, to_add: Segment) {
        if !self
            .segments
            .iter()
            .any(|segment| segment.index == to_add.index)
        {
            self.actual_size += to_add.data.len();
            self.segments.push(to_add);
        }
    }

    pub fn collect_file(&self) -> Vec<u8> {
        let mut segments = self.segments.clone();
        segments.sort_by(|a, b| a.index.cmp(&b.index));
        segments
            .into_iter()
            .map(|segment| segment.data)
            .collect::<Vec<_>>()
            .concat()
    }

    fn is_complete(&self) -> bool {
        if self.intended_size != self.actual_size {
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
