use crate::{NamedRequest, Request, TorrentFile};
use std::error::Error;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub(crate) struct PeerClient {
    pub(super) files: Vec<TorrentFile>,
    tracker: SocketAddr,
}

impl PeerClient {
    pub fn new(folder: PathBuf, tracker: SocketAddr) -> Self {
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

        PeerClient { files, tracker }
    }

    pub(super) async fn download_file_peer(
        &mut self,
        name: String,
        socket: SocketAddr,
    ) -> Result<(), Box<dyn Error>> {
        // let mut rx: BincodeReceiver<NamedRequest> = receiver_on(socket).await?;

        let mut stream = TcpStream::connect(socket).await?;

        let (mut reader, mut writer) = stream.split();

        let fetch_numbers = bincode::serialize(&NamedRequest {
            name: name.to_string(),
            request: Request::FetchFileInfo,
        })
        .unwrap(); // unwrap?

        writer.write_all(&fetch_numbers).await?;
        writer.flush().await?;

        //writer.shutdown().await?;
        reader.readable().await?;

        self.process_stream(&mut stream).await?;

        Ok(())
    }

    pub async fn download_file(&mut self, name: String) -> io::Result<()> {
        let peer_sockets = Self::fetch_peers(self.tracker).await?;
        for socket in peer_sockets {
            let name = name.clone();
            self.download_file_peer(name, socket).await.unwrap();
        }
        Ok(())
    }

    fn find_file(&self, name: &str) -> Option<&TorrentFile> {
        self.files.iter().find(|file| file.name == name)
    }

    fn create_file(&mut self, name: &str) {
        self.files.push(TorrentFile::new(vec![], name));
    }

    async fn fetch_peers(tracker: SocketAddr) -> io::Result<Vec<SocketAddr>> {
        unimplemented!()
    }

    pub(super) async fn process_stream(&mut self, mut stream: &mut TcpStream) -> io::Result<()> {
        loop {
            let mut buffer = vec![];
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
                Request::Finished => {
                    break;
                }
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

    fn create_empty_file(&mut self, name: &str, size: usize) {
        self.files.push(TorrentFile {
            segments: vec![],
            name: name.to_string(),
            size,
        })
    }

    async fn send_request(request: NamedRequest, stream: &mut TcpStream) -> io::Result<()> {
        let to_send = bincode::serialize(&request).unwrap();
        let (mut reader, mut writer) = stream.split();
        writer.write_all(&to_send).await?;
        writer.flush().await?;
        writer.shutdown().await
    }
}
