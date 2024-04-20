use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::Path;
use tokio::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const SEGMENT_SIZE: usize = 256 * 1024;

#[derive(Serialize, Deserialize)]
struct TorrentFile {
    segments: Vec<Segment>,
    name: String,
    size: usize,
}

impl TorrentFile {
    fn new(data: Vec<u8>, name: &str) -> Self {
        let mut index = 0;
        let mut read = 0;
        let mut segments = Vec::with_capacity(data.len() / SEGMENT_SIZE + 1);
        while read < data.len() && (index + 1) * SEGMENT_SIZE < data.len() {
            let segment = Segment { index, data: data[index * SEGMENT_SIZE..(index + 1) * SEGMENT_SIZE].to_vec() }; // double clone
            segments.push(segment);

            read += SEGMENT_SIZE;
            index += 1;
        }

        if read > data.len() {
            read -= SEGMENT_SIZE;
            let segment = Segment { index, data: data[index * SEGMENT_SIZE..data.len()].to_vec() };
            segments.push(segment);
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
        self.segments.sort_by(|a, b| a.index.cmp(&b.index));
        self.segments.iter().map(|segment| segment.data.clone()).collect::<Vec<_>>().concat()
    }
}

#[derive(Serialize, Deserialize)]
struct NamedRequest {
    name: String,
    request: Request,
}

impl NamedRequest {
    fn new(name: String, request: Request) -> Self {
        Self { name, request }
    }
}

#[derive(Serialize, Deserialize)]
enum Request {
    FetchNumbers,
    FetchSegment { seg_number: usize },
    ReceiveNumbers { numbers: Option<Vec<usize>> },
    ReceiveSegment { segment: Segment },
}

struct Peer {
    files: Vec<TorrentFile>,
    tracker: SocketAddr, // peers: Vec<SocketAddr> - should be session-wide only
}

#[derive(Serialize, Deserialize, Clone)]
struct Segment {
    index: usize,
    data: Vec<u8>,
}

impl Peer {
    fn find_file(&self, name: &str) -> Option<&TorrentFile> {
        self.files.iter().find(|file| file.name == name)
    }

    pub fn new(folder: &Path, tracker: SocketAddr) -> Self {
        let file_paths = std::fs::read_dir(folder)
            .unwrap()
            .map(|dir| dir.unwrap().path())
            .filter(|dir| dir.is_file())
            .collect::<Vec<_>>();

        let files = file_paths.into_iter().map(|path| {
            let data = std::fs::read(&path).unwrap();
            let name = path.file_name().unwrap().to_str().unwrap();
            TorrentFile::new(data, name)
        }).collect();

        let mut peer = Peer {
            files,
            tracker,
        };

        tokio::spawn(async move {
            peer.listen()
        });

        peer
    }

    async fn download_file_peer(&mut self, name: &str, socket: SocketAddr) -> io::Result<()> {
        let mut stream = TcpStream::connect(socket).await?;
        let (mut reader, mut writer) = stream.split();

        let fetch_numbers = bincode::serialize(&NamedRequest {
            name: name.to_string(),
            request: Request::FetchNumbers,
        })
            .unwrap(); // unwrap?

        writer.write_all(&fetch_numbers).await?;

        Ok(())
    }

    pub async fn download_file(&mut self, name: &str) -> io::Result<()> {
        let peer_sockets = Self::fetch_peers(self.tracker).await?;
        for socket in peer_sockets {
            let mut stream = TcpStream::connect(socket).await?;
            let (mut reader, mut writer) = stream.split();

            let fetch_numbers = bincode::serialize(&NamedRequest {
                name: name.to_string(),
                request: Request::FetchNumbers,
            })
            .unwrap(); // unwrap?

            writer.write_all(&fetch_numbers).await?;
        }
        Ok(())
    }

    async fn listen(&mut self) -> io::Result<()> {
        let listener = TcpListener::bind("127.0.0.1:8000").await?;

        loop {
            let (mut stream, socket) = listener.accept().await?;
            let mut buffer = vec![];

            stream.read_to_end(&mut buffer).await?;
            let request: NamedRequest = bincode::deserialize(&buffer).unwrap(); // unwrap! very scary

            match request.request {
                Request::FetchNumbers => {
                    let numbers = self.find_file(&request.name).and_then(|file| {
                        Some(file.segments.iter().map(|segment| segment.index).collect())
                    });

                    let request =
                        NamedRequest::new(request.name, Request::ReceiveNumbers { numbers });

                    Self::send_request(request, &mut stream).await?;
                }
                Request::FetchSegment { seg_number } => {
                    let segment = self.find_file(&request.name).and_then(|file| {
                        file.segments
                            .iter()
                            .find(|segment| segment.index == seg_number)
                    });

                    let request = NamedRequest::new(
                        request.name,
                        Request::ReceiveSegment {
                            segment: segment.unwrap().clone(),
                        },
                    );

                    Self::send_request(request, &mut stream).await?;
                }
                Request::ReceiveNumbers { numbers } => match numbers {
                    None => {}
                    Some(numbers) => {
                        let segments = self
                            .find_file(&request.name)
                            .and_then(|file| {
                                Some(
                                    file.segments
                                        .iter()
                                        .zip(numbers.iter())
                                        .filter(|(segment, seg_number)| {
                                            segment.index == **seg_number
                                        })
                                        .map(|(segment, _)| segment)
                                        .collect::<Vec<_>>(),
                                )
                            })
                            .unwrap();

                        for segment in segments.into_iter() {
                            let request = NamedRequest::new(
                                request.name.clone(),
                                Request::ReceiveSegment {
                                    segment: segment.clone(),
                                },
                            );

                            Self::send_request(request, &mut stream).await?;
                        }
                    }
                },
                Request::ReceiveSegment { segment } => {
                    self.files
                        .iter_mut()
                        .find(|file| file.name == request.name)
                        .and_then(|file| Some(file.add_segment(segment)));
                }
            }
        }
    }

    async fn send_request(request: NamedRequest, stream: &mut TcpStream) -> io::Result<()> {
        let to_send = bincode::serialize(&request).unwrap();
        stream.write_all(&to_send).await
    }

    async fn fetch_peers(tracker: SocketAddr) -> io::Result<Vec<SocketAddr>> {
        unimplemented!()
    }
}

mod tests {
    use std::net::SocketAddr;
    use std::path::Path;
    use crate::Peer;

    #[tokio::test]
    async fn download_from_one_peer_test() {
        let mut peer1 = Peer::new(Path::new("../.."), SocketAddr::new("127.0.0.1".parse().unwrap(), 8000));
        let peer2 = Peer::new(Path::new("../../src"), SocketAddr::new("127.0.0.1".parse().unwrap(), 8001));

        peer1.download_file_peer("file.pdf", SocketAddr::new("127.0.0.1".parse().unwrap(), 8001)).await.unwrap();

        let main = std::fs::read("main.rs").unwrap();
        assert_eq!(main, peer1.files[0].collect_file())
    }
}
