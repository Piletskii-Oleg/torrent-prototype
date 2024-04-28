use std::cmp::min;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::Segment;
use crate::storage::{SEGMENT_SIZE, Storage};

pub struct MemoryStorage(Vec<TorrentFile>);

#[derive(Serialize, Deserialize, Clone)]
pub struct TorrentFile {
    segments: Vec<Segment>,
    name: String,
    intended_size: usize,
    actual_size: usize,
}

impl Storage for MemoryStorage {
    fn add_segment(&mut self, segment: Segment) -> std::io::Result<()> {
        todo!()
    }

    fn is_complete(&self, name: &str) -> bool {
        todo!()
    }

    fn create_empty_file(&mut self) -> std::io::Result<()> {
        todo!()
    }

    fn file_data(&self) -> Vec<u8> {
        todo!()
    }

    fn file_size(&self, name: &str) -> Option<usize> {
        todo!()
    }

    fn path(&self, name: &str) -> PathBuf {
        todo!()
    }

    fn segment_numbers(&self, name: &str) -> Option<Vec<usize>> {
        todo!()
    }

    fn segment(&self, name: &str, index: usize) -> Segment {
        todo!()
    }
}

impl MemoryStorage {
    pub fn new(folder: PathBuf) -> Self {
        let file_paths = std::fs::read_dir(&folder)
            .unwrap()
            .map(|dir| dir.unwrap().path())
            .filter(|dir| dir.is_file())
            .collect::<Vec<_>>();

        let files = file_paths
            .into_iter()
            .map(|path| {
                let data = std::fs::read(&path).unwrap();
                let name = path.file_name().unwrap().to_str().unwrap();
                TorrentFile::new(data, name)
            })
            .collect();

        Self(files)
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
            };
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
