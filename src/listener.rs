use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use crate::{NamedRequest, Request, TorrentFile};

pub(super) struct PeerListener {
    files: Vec<TorrentFile>,
    listen_address: SocketAddr, // peers: Vec<SocketAddr> - should be session-wide only
}

impl PeerListener {
    fn find_file(&self, name: &str) -> Option<&TorrentFile> {
        self.files.iter().find(|file| file.name == name)
    }

    pub(super) fn new_listen(folder: PathBuf, listen_address: SocketAddr) {
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

        let mut peer = PeerListener {
            files,
            listen_address,
        };

        tokio::spawn(async move {
            peer.listen(listen_address).await
        });
    }

    fn create_file(&mut self, name: &str) {
        self.files.push(TorrentFile::new(vec![], name));
    }

    async fn listen(mut self, addr: SocketAddr) -> io::Result<()> {
        let listener = TcpListener::bind(addr).await?;

        loop {
            let (mut stream, socket) = listener.accept().await?;
            let mut buffer = vec![];

            stream.read_to_end(&mut buffer).await?;
            let request: NamedRequest = bincode::deserialize(&buffer).unwrap(); // unwrap! very scary

            match request.request {
                Request::FetchNumbers => {
                    if self.find_file(&request.name).is_none() {
                        self.create_file(&request.name);
                    }
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
                    let a = self.files
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
}
