use crate::{NamedRequest, Request, TorrentFile};
use std::fmt::Debug;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

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

        let files = file_paths
            .into_iter()
            .map(|path| {
                let data = std::fs::read(&path).unwrap();
                let name = path.file_name().unwrap().to_str().unwrap();
                TorrentFile::new(data, name)
            })
            .collect();

        let mut peer = PeerListener {
            files,
            listen_address,
        };

        tokio::spawn(async move { peer.listen(listen_address).await });
    }

    pub(super) async fn from_stream(files: &[TorrentFile], stream: TcpStream) {
        let mut peer = PeerListener {
            files: files.to_vec(),
            listen_address: stream.local_addr().unwrap(),
        };

        peer.process_stream(stream).await.unwrap();
    }

    fn create_empty_file(&mut self, name: &str, size: usize) {
        self.files.push(TorrentFile {
            segments: vec![],
            name: name.to_string(),
            size,
        })
    }

    async fn listen(mut self, addr: SocketAddr) -> io::Result<()> {
        let listener = TcpListener::bind(addr).await?;

        loop {
            let (stream, _) = listener.accept().await?;
            println!("listener local: {}", stream.local_addr().unwrap());
            println!("listener remote: {}", stream.peer_addr().unwrap());
            self.process_stream(stream).await?;
        }
    }

    pub(super) async fn process_stream(&mut self, mut stream: TcpStream) -> io::Result<()> {
        loop {
            let mut buffer = vec![];
            stream.readable().await?;
            stream.read_to_end(&mut buffer).await?;
            if buffer.is_empty() {
                continue;
            }

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
                            println!("recv seg {}", segment.index);
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
                    let a = self
                        .files
                        .iter_mut()
                        .find(|file| file.name == request.name)
                        .and_then(|file| Some(file.add_segment(segment)));
                    println!("{a:?}");

                    let file = self.find_file(&request.name).unwrap();
                    if file.is_complete() {
                        Self::send_request(
                            NamedRequest::new(request.name, Request::Finished),
                            &mut stream,
                        )
                            .await?;
                    }
                }
                Request::Finished => { break; }
                Request::FetchFileInfo => match self.find_file(&request.name) {
                    None => {}
                    Some(file) => {
                        let request = NamedRequest::new(
                            request.name,
                            Request::ReceiveFileInfo { size: file.size },
                        );
                        Self::send_request(request, &mut stream).await?;
                    }
                },
                Request::ReceiveFileInfo { size } => {
                    self.create_empty_file(&request.name, size);
                    let request = NamedRequest::new(request.name, Request::FetchNumbers);
                    Self::send_request(request, &mut stream).await?;
                }
            }
        }
        Ok(())
    }

    async fn send_request(request: NamedRequest, stream: &mut TcpStream) -> io::Result<()> {
        let to_send = bincode::serialize(&request).unwrap();
        let (mut reader, mut writer) = stream.split();
        writer.write_all(&to_send).await?;
        writer.flush().await?;
        writer.shutdown().await
    }
}
