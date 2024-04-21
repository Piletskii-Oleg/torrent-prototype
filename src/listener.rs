use std::error::Error;
use crate::{NamedRequest, Request, TorrentFile};
use std::fmt::Debug;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tsyncp::channel::{BincodeChannel, channel_on, channel_to};

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

    fn create_empty_file(&mut self, name: &str, size: usize) {
        self.files.push(TorrentFile {
            segments: vec![],
            name: name.to_string(),
            size,
        })
    }

    async fn listen(mut self, addr: SocketAddr) -> io::Result<()> {
        //let listener = TcpListener::bind(addr).await?;

        let channel = channel_on(addr).await.unwrap();
        self.process_stream(channel).await

        // loop {
        //     let socket = {
        //         let (str, socket) = listener.accept().await?;
        //         socket
        //     };
        //     let channel = channel_on(socket).await.unwrap();
        //     self.process_stream(channel).await?;
        // }
    }

    pub(super) async fn process_stream(&mut self, mut channel: BincodeChannel<NamedRequest>) -> io::Result<()> {
        loop {
            let request = channel.recv().await.unwrap().unwrap();

            match request.request {
                Request::FetchNumbers => {
                    let numbers = self.find_file(&request.name).and_then(|file| {
                        Some(file.segments.iter().map(|segment| segment.index).collect())
                    });

                    let request =
                        NamedRequest::new(request.name, Request::ReceiveNumbers { numbers });

                    channel.send(request).await.unwrap();
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

                    channel.send(request).await.unwrap();
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

                            channel.send(request).await.unwrap();
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
                        channel.send(NamedRequest::new(request.name, Request::Finished)).await.unwrap();
                    }
                }
                Request::Finished => {
                    channel.send(NamedRequest::new(request.name, Request::Finished)).await.unwrap();
                    break;
                }
                Request::FetchFileInfo => match self.find_file(&request.name) {
                    None => {}
                    Some(file) => {
                        let request = NamedRequest::new(
                            request.name,
                            Request::ReceiveFileInfo { size: file.size },
                        );
                        channel.send(request).await.unwrap();
                    }
                },
                Request::ReceiveFileInfo { size } => {
                    self.create_empty_file(&request.name, size);
                    let request = NamedRequest::new(request.name, Request::FetchNumbers);
                    channel.send(request).await.unwrap();
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
