mod memory;

use crate::Segment;
use std::io;
use std::path::PathBuf;

pub use memory::MemoryStorage;

const KB: usize = 1024;

const SEGMENT_SIZE: usize = 256 * KB;

pub trait Storage {
    fn add_segment(&mut self, name: &str, segment: Segment) -> io::Result<()>;
    fn is_complete(&self, name: &str) -> bool;
    fn create_empty_file(&mut self, name: &str, size: usize) -> io::Result<()>;
    fn file_data(&self, name: &str) -> Vec<u8>;
    fn file_size(&self, name: &str) -> Option<usize>;
    fn path(&self, name: &str) -> PathBuf;
    fn segment_numbers(&self, name: &str) -> Option<Vec<usize>>;
    fn segment(&self, name: &str, index: usize) -> Segment;
}
