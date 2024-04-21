use crate::{NamedRequest, OldRequest, PeerListener, TorrentFile};
use std::error::Error;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpSocket, TcpStream};
use tsyncp::channel::{channel_to, BincodeChannel};

const NOTIFY_STRING: &str = "SET_FILE";

const FETCH_FILE_STRING: &str = "EXIST_FILE";

const FETCH_FILE_PEERS_STRING: &str = "GET_PEERS";

pub struct PeerClient {
    pub files: Arc<Mutex<Vec<TorrentFile>>>,
    tracker: SocketAddr,
}

impl PeerClient {
    pub async fn new(folder: PathBuf, tracker: SocketAddr, listen_address: SocketAddr) -> Self {
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

        let files = Arc::new(Mutex::new(files));
        let peer = PeerClient {
            files,
            tracker,
        };

        PeerListener::new_listen(peer.files.clone(), listen_address);

        //peer.notify_tracker().await.unwrap();

        peer
    }

    pub async fn download_file_peer(
        &mut self,
        name: String,
        socket: SocketAddr,
    ) -> Result<(), Box<dyn Error>> {
        println!("Connecting to channel with {socket}");
        let mut channel: BincodeChannel<NamedRequest> = channel_to(socket).await?;

        let fetch_file = NamedRequest {
            name: name.to_string(),
            request: OldRequest::FetchFileInfo,
        };

        channel.send(fetch_file).await?;

        self.process_channel(channel).await?;

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

    fn create_file(&mut self, name: &str) {
        self.files
            .lock()
            .unwrap()
            .push(TorrentFile::new(vec![], name));
    }

    async fn fetch_peers(&self, file_name: &str) -> Result<Vec<SocketAddr>, Box<dyn Error>> {
        let request = format!("{FETCH_FILE_PEERS_STRING}\n{file_name}");

        println!("Requesting peers:\n{request}");
        let mut tracker = TcpStream::connect(self.tracker).await?;
        tracker.write_all(request.as_bytes()).await?;
        tracker.flush().await?;
        tracker.shutdown().await?;

        let mut received = String::new();
        tracker.read_to_string(&mut received).await?;

        println!("Received peers:\n{received}");

        Ok(received
            .lines()
            .skip(1)
            .map(|ip| ip.parse().unwrap())
            .collect())
    }

    async fn notify_tracker(&self) -> io::Result<()> {
        let requests = self
            .files
            .lock()
            .unwrap()
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

    async fn process_channel(
        &mut self,
        mut channel: BincodeChannel<NamedRequest>,
    ) -> Result<(), Box<dyn Error>> {
        loop {
            let request = channel.recv().await.unwrap()?;

            match request.request {
                OldRequest::ReceiveNumbers { numbers } => match numbers {
                    None => {}
                    Some(numbers) => {
                        println!(
                            "Client: Received numbers from peer on {}",
                            channel.peer_addr()
                        );
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
                    println!(
                        "Client: Received segment number {} from peer on {}",
                        segment.index,
                        channel.peer_addr()
                    );
                    let index = segment.index;
                    self.files
                        .lock()
                        .unwrap()
                        .iter_mut()
                        .find(|file| file.name == request.name)
                        .and_then(|file| Some(file.add_segment(segment)));

                    println!(
                        "Client: Added segment number {} from {} to the file {}.",
                        index,
                        channel.peer_addr(),
                        request.name
                    );
                    let guard = self.files.lock().unwrap();
                    let file = guard.iter().find(|file| file.name == request.name).unwrap();
                    if file.is_complete() {
                        channel
                            .send(NamedRequest::new(request.name, OldRequest::Finished))
                            .await?;
                    }
                }
                OldRequest::Finished => {
                    println!("Client: {}: download complete.", request.name);
                    let data = self
                        .files
                        .lock()
                        .unwrap()
                        .iter_mut()
                        .find(|file| file.name == request.name)
                        .unwrap()
                        .collect_file();
                    std::fs::write(&request.name, data)?;
                    channel
                        .send(NamedRequest::new(request.name, OldRequest::Finished))
                        .await?;
                    break;
                }
                OldRequest::ReceiveFileInfo { size } => {
                    println!("Client: Received {}'s size: {size}", request.name);
                    self.create_empty_file(&request.name, size);
                    let request = NamedRequest::new(request.name, OldRequest::FetchNumbers);
                    channel.send(request).await?;
                }
                _ => unreachable!(),
            }
        }
        Ok(())
    }

    fn create_empty_file(&mut self, name: &str, size: usize) {
        self.files.lock().unwrap().push(TorrentFile {
            segments: vec![],
            name: name.to_string(),
            size,
        })
    }
}
