use crate::{NamedRequest, OldRequest, TorrentFile};
use std::error::Error;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tsyncp::channel::{channel_to, BincodeChannel};

const NOTIFY_STRING: &str = "SET_FILE";

const FETCH_FILE_STRING: &str = "EXIST_FILE";

const FETCH_FILE_PEERS_STRING: &str = "GET_PEERS";

pub(crate) struct PeerClient {
    pub(super) files: Vec<TorrentFile>,
    tracker: SocketAddr,
}

impl PeerClient {
    pub async fn new(folder: PathBuf, tracker: SocketAddr) -> Self {
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

        let peer = PeerClient { files, tracker };

        //peer.notify_tracker().await.unwrap();

        peer
    }

    pub(super) async fn download_file_peer(
        &mut self,
        name: String,
        socket: SocketAddr,
    ) -> Result<(), Box<dyn Error>> {
        let mut channel: BincodeChannel<NamedRequest> =
            channel_to(socket).retry(Duration::from_secs(1), 5).await?;

        let fetch_numbers = NamedRequest {
            name: name.to_string(),
            request: OldRequest::FetchFileInfo,
        };

        channel.send(fetch_numbers).await?;

        self.process_stream(channel).await?;

        Ok(())
    }

    pub async fn download_file(&mut self, name: String) -> Result<(), Box<dyn Error>> {
        let peer_sockets = self.fetch_peers(&name).await?;
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

    async fn fetch_peers(&self, file_name: &str) -> Result<Vec<SocketAddr>, Box<dyn Error>> {
        let request = format!("{FETCH_FILE_PEERS_STRING}\n{file_name}");

        let mut tracker = TcpStream::connect(self.tracker).await?;
        tracker.write_all(request.as_bytes()).await?;
        tracker.flush().await?;
        tracker.shutdown().await?;

        let mut received = String::new();
        tracker.read_to_string(&mut received).await?;

        Ok(received.lines().skip(1).map(|ip| ip.parse().unwrap()).collect())
    }

    async fn notify_tracker(&self) -> io::Result<()> {
        let requests = self.files
            .iter()
            .map(|file| file.name.clone())
            .map(|name| format!("{NOTIFY_STRING}\n{name}"))
            .collect::<Vec<_>>();

        let mut tracker = TcpStream::connect(self.tracker).await?;
        for request in requests {
            tracker.write_all(request.as_bytes()).await?;
            tracker.flush().await?;
        }

        Ok(())
    }

    async fn process_stream(
        &mut self,
        mut channel: BincodeChannel<NamedRequest>,
    ) -> Result<(), Box<dyn Error>> {
        loop {
            let request = channel.recv().await.unwrap()?;

            match request.request {
                OldRequest::FetchNumbers => {
                    let numbers = self.find_file(&request.name).and_then(|file| {
                        Some(file.segments.iter().map(|segment| segment.index).collect())
                    });

                    let request =
                        NamedRequest::new(request.name, OldRequest::ReceiveNumbers { numbers });

                    channel.send(request).await?;
                }
                OldRequest::FetchSegment { seg_number } => {
                    let segment = self.find_file(&request.name).and_then(|file| {
                        file.segments
                            .iter()
                            .find(|segment| segment.index == seg_number)
                    });

                    let request = NamedRequest::new(
                        request.name,
                        OldRequest::ReceiveSegment {
                            segment: segment.unwrap().clone(),
                        },
                    );

                    channel.send(request).await?;
                }
                OldRequest::ReceiveNumbers { numbers } => match numbers {
                    None => {}
                    Some(numbers) => {
                        println!("Client: Received numbers from peer on {}", channel.peer_addr());
                        for seg_number in numbers {
                            let request = NamedRequest::new(
                                request.name.clone(),
                                OldRequest::FetchSegment { seg_number },
                            );
                            channel.send(request).await?;
                        }
                    }
                },
                OldRequest::ReceiveSegment { segment } => {
                    println!("Client: Received segment number {} from peer on {}", segment.index, channel.peer_addr());
                    let index = segment.index;
                    self.files
                        .iter_mut()
                        .find(|file| file.name == request.name)
                        .and_then(|file| Some(file.add_segment(segment)));

                    println!("Client: Added segment number {} from {} to the file {}.", index, channel.peer_addr(), request.name);
                    let file = self.find_file(&request.name).unwrap();
                    if file.is_complete() {
                        channel
                            .send(NamedRequest::new(request.name, OldRequest::Finished))
                            .await?;
                    }
                }
                OldRequest::Finished => {
                    println!("Client: {}: download complete.", request.name);
                    channel
                        .send(NamedRequest::new(request.name, OldRequest::Finished))
                        .await?;
                    break;
                }
                OldRequest::FetchFileInfo => match self.find_file(&request.name) {
                    None => {}
                    Some(file) => {
                        let request = NamedRequest::new(
                            request.name,
                            OldRequest::ReceiveFileInfo { size: file.size },
                        );
                        channel.send(request).await?;
                    }
                },
                OldRequest::ReceiveFileInfo { size } => {
                    println!("Client: Received {}'s size: {size}", request.name);
                    self.create_empty_file(&request.name, size);
                    let request = NamedRequest::new(request.name, OldRequest::FetchNumbers);
                    channel.send(request).await?;
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
}
