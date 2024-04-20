use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Serialize, Deserialize)]
struct TorrentFile {
    segments: Vec<Segment>,
    name: String,
    size: usize,
}

impl TorrentFile {
    fn add_segment(&mut self, to_add: Segment) {
        if self.segments.iter().find(|segment| segment.number == to_add.number).is_none() {
            self.segments.push(to_add);
        }
    }
}

struct NamedRequest {
    name: String,
    request: Request,
}

#[derive(Serialize, Deserialize)]
enum Request {
    FetchNumbers {
        name: String,
    },
    FetchSegment {
        name: String,
        seg_number: u32,
    },
    ReceiveNumbers {
        name: String,
        numbers: Option<Vec<u32>>,
    },
    ReceiveSegment {
        name: String,
        segment: Segment,
    },
}

struct Peer {
    files: Vec<TorrentFile>,
    tracker: SocketAddr, // peers: Vec<SocketAddr> - should be session-wide only
}

#[derive(Serialize, Deserialize, Clone)]
struct Segment {
    number: u32,
    data: Vec<u8>,
}

impl Peer {
    pub async fn new(folder: &Path, tracker: SocketAddr) -> Self {
        Peer {
            files: vec![],
            tracker,
        } // read files from folder
    }

    pub async fn download_file(&mut self, name: &str) -> tokio::io::Result<()> {
        let peer_sockets = Self::fetch_peers(self.tracker).await?;
        for socket in peer_sockets {
            let mut stream = TcpStream::connect(socket).await?;
            let (mut reader, mut writer) = stream.split();

            let request = bincode::serialize(&Request::FetchNumbers {
                name: name.to_string(),
            })
            .unwrap(); // unwrap?

            writer.write_all(&request).await?;
        }
        Ok(())
    }

    async fn listen(&mut self) -> tokio::io::Result<()> {
        let listener = TcpListener::bind("127.0.0.1:8000").await?;

        loop {
            let (mut stream, socket) = listener.accept().await?;
            let mut buffer = vec![];

            stream.read_to_end(&mut buffer).await?;
            let request: Request = bincode::deserialize(&buffer).unwrap(); // unwrap! very scary

            match request {
                Request::FetchNumbers { name } => {
                    let numbers =
                        self.files
                            .iter()
                            .find(|file| file.name == name)
                            .and_then(|file| {
                                Some(file.segments.iter().map(|segment| segment.number).collect())
                            });

                    let to_send =
                        bincode::serialize(&Request::ReceiveNumbers { name, numbers }).unwrap();
                    stream.write_all(&to_send).await?;
                }
                Request::FetchSegment { name, seg_number } => {
                    let segment = self
                        .files
                        .iter()
                        .find(|file| file.name == name)
                        .and_then(|file| {
                            file.segments
                                .iter()
                                .find(|segment| segment.number == seg_number)
                        })
                        .unwrap()
                        .clone();
                    let to_send = bincode::serialize(&Request::ReceiveSegment {
                        name, segment: segment.to_owned(),
                    })
                    .unwrap();
                    stream.write_all(&to_send).await?;
                }
                Request::ReceiveNumbers { name, numbers } => match numbers {
                    None => {}
                    Some(numbers) => {
                        let segments = self
                            .files
                            .iter()
                            .find(|file| &file.name == &name)
                            .and_then(|file| {
                                Some(file.segments
                                    .iter()
                                    .zip(numbers.iter())
                                    .filter(|(segment, seg_number)| segment.number == **seg_number)
                                    .map(|(segment, _)| segment)
                                    .collect::<Vec<_>>())
                            }).unwrap();

                        for segment in segments.into_iter() {
                            let to_send = bincode::serialize(&Request::ReceiveSegment { name: name.clone(), segment: segment.clone() }).unwrap();
                            stream.write_all(&to_send).await?;
                        }
                    }
                },
                Request::ReceiveSegment { segment, name } => {
                    self.files.iter_mut().find(|file| file.name == name)
                        .and_then(|file| Some(file.add_segment(segment)));
                }
            }
        }
    }

    pub async fn send_segment(name: &str, seg_number: usize) -> tokio::io::Result<()> {
        unimplemented!()
    }

    async fn fetch_peers(tracker: SocketAddr) -> tokio::io::Result<Vec<SocketAddr>> {
        unimplemented!()
    }
}
