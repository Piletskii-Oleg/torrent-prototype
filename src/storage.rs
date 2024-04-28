mod memory;

use std::io;
use std::path::PathBuf;
use crate::Segment;

pub use memory::MemoryStorage;

const KB: usize = 1024;

const SEGMENT_SIZE: usize = 256 * KB;

pub trait Storage {
    fn add_segment(&mut self, segment: Segment) -> io::Result<()>;
    fn is_complete(&self, name: &str) -> bool;
    fn create_empty_file(&mut self) -> io::Result<()>;
    fn file_data(&self) -> Vec<u8>;
    fn file_size(&self, name: &str) -> Option<usize>;
    fn path(&self, name: &str) -> PathBuf;
    fn segment_numbers(&self, name: &str) -> Option<Vec<usize>>;
    fn segment(&self, name: &str, index: usize) -> Segment;
}

