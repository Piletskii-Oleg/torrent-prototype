use crate::{Fetch, NamedRequest, PeerListener, Receive, Request, TorrentFile};
use std::error::Error;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::io;
use tokio::task::JoinHandle;
use tsyncp::channel::{channel_to, BincodeChannel};

const NOTIFY_STRING: &str = "SET_FILE";

#[allow(dead_code)]
const FETCH_FILE_STRING: &str = "EXIST_FILE";

const FETCH_FILE_PEERS_STRING: &str = "GET_PEERS";

pub struct PeerClient {
    folder: PathBuf,
    files: Arc<Mutex<Vec<TorrentFile>>>,
    // tracker: SocketAddr,
}

impl PeerClient {
    pub async fn new(folder: PathBuf, listen_address: SocketAddr) -> Self {
        let file_paths = std::fs::read_dir(&folder)
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
            folder,
            files,
            // tracker,
        };

        PeerListener::listen(peer.files.clone(), listen_address);

        //peer.notify_tracker().await.unwrap();

        peer
    }

    pub async fn download_file_peer(
        name: String,
        socket: SocketAddr,
        files: Arc<Mutex<Vec<TorrentFile>>>,
        folder: PathBuf,
    ) -> Result<(), Box<dyn Error>> {
        println!("Connecting to channel with {socket}");
        let mut channel: BincodeChannel<NamedRequest> = channel_to(socket).await?;

        let fetch_file = NamedRequest {
            name: name.to_string(),
            request: Fetch::FileInfo.into(),
        };

        channel.send(fetch_file).await?;

        Self::process_channel(files, folder, channel).await?;

        Ok(())
    }

    pub async fn download_file(
        &mut self,
        name: String,
    ) -> Result<Vec<JoinHandle<()>>, Box<dyn Error>> {
        let peer_sockets = self.fetch_peers(&name).await?;
        let mut handles = Vec::with_capacity(peer_sockets.len());
        for socket in peer_sockets {
            let folder = self.folder.clone();
            let files = self.files.clone();
            let name = name.clone();
            handles.push(tokio::spawn(async move {
                let name = name.clone();
                Self::download_file_peer(name, socket, files, folder)
                    .await
                    .unwrap();
            }));
        }
        Ok(handles)
    }

    async fn fetch_peers(&self, file_name: &str) -> Result<Vec<SocketAddr>, Box<dyn Error>> {
        Ok(vec![SocketAddr::new("127.0.0.1".parse().unwrap(), 8001)])
        // let request = format!("{FETCH_FILE_PEERS_STRING}\n{file_name}");
        //
        // println!("Requesting peers:\n{request}");
        // let mut tracker = TcpStream::connect(self.tracker).await?;
        // tracker.write_all(request.as_bytes()).await?;
        // tracker.flush().await?;
        // tracker.shutdown().await?;
        //
        // let mut received = String::new();
        // tracker.read_to_string(&mut received).await?;
        //
        // println!("Received peers:\n{received}");
        //
        // Ok(received
        //     .lines()
        //     .skip(1)
        //     .map(|ip| ip.parse().unwrap())
        //     .collect())
    }

    async fn notify_tracker(&self) -> io::Result<()> {
        // let requests = self
        //     .files
        //     .lock()
        //     .unwrap()
        //     .iter()
        //     .map(|file| file.name.clone())
        //     .map(|name| format!("{NOTIFY_STRING}\n{name}"))
        //     .collect::<Vec<_>>();
        //
        // let mut tracker = TcpStream::connect(self.tracker).await?;
        // for request in requests {
        //     tracker.write_all(request.as_bytes()).await?;
        //     tracker.flush().await?;
        // }

        Ok(())
    }

    async fn process_channel(
        files: Arc<Mutex<Vec<TorrentFile>>>,
        folder: PathBuf,
        mut channel: BincodeChannel<NamedRequest>,
    ) -> Result<(), Box<dyn Error>> {
        loop {
            let request = channel.recv().await.unwrap()?;

            match request.request {
                Request::Client(client_request) => {
                    Self::process_client(files.clone(), client_request, &mut channel, request.name)
                        .await?
                }
                Request::Listener(_) => unreachable!(),
                Request::Finished => {
                    println!("Client: {}: download complete.", request.name);

                    let data = files
                        .lock()
                        .unwrap()
                        .iter_mut()
                        .find(|file| file.name == request.name)
                        .unwrap()
                        .collect_file();
                    let path = folder.join(&request.name);
                    std::fs::write(path, data)?;

                    channel
                        .send(NamedRequest::new(request.name, Request::Finished))
                        .await?;
                    break;
                }
            };
        }
        Ok(())
    }

    async fn process_client(
        files: Arc<Mutex<Vec<TorrentFile>>>,
        request: Receive,
        channel: &mut BincodeChannel<NamedRequest>,
        name: String,
    ) -> Result<(), Box<dyn Error>> {
        match request {
            Receive::Numbers(numbers) => match numbers {
                None => Ok(()),
                Some(numbers) => {
                    println!(
                        "Client: Received numbers from peer on {}",
                        channel.peer_addr()
                    );
                    for seg_number in numbers {
                        let request = NamedRequest::new(
                            name.clone(),
                            Fetch::SegmentNumber(seg_number).into(),
                        );
                        channel.send(request).await?;
                    }
                    Ok(())
                }
            },
            Receive::Segment(segment) => {
                println!(
                    "Client: Received segment number {} from peer on {}",
                    segment.index,
                    channel.peer_addr()
                );
                let index = segment.index;
                if let Some(file) = files
                    .lock()
                    .unwrap()
                    .iter_mut()
                    .find(|file| file.name == name)
                {
                    file.add_segment(segment)
                }

                println!(
                    "Client: Added segment number {} from {} to the file {}.",
                    index,
                    channel.peer_addr(),
                    name
                );

                let is_complete = {
                    let guard = files.lock().unwrap();
                    let file = guard.iter().find(|file| file.name == name).unwrap();
                    file.is_complete()
                };
                if is_complete {
                    channel
                        .send(NamedRequest::new(name, Request::Finished))
                        .await?;
                }
                Ok(())
            }
            Receive::FileSize(size) => {
                println!("Client: Received {name}'s size: {size}");
                Self::create_empty_file(files, &name, size);
                let request = NamedRequest::new(name, Fetch::Numbers.into());
                channel.send(request).await?;
                Ok(())
            }
        }
    }

    fn create_empty_file(files: Arc<Mutex<Vec<TorrentFile>>>, name: &str, size: usize) {
        let empty_file = TorrentFile::new_empty(name, size);
        files.lock().unwrap().push(empty_file)
    }

    pub fn file_data(&self, name: &str) -> Option<Vec<u8>> {
        Some(
            self.files
                .lock()
                .unwrap()
                .iter()
                .find(|file| file.name == name)
                .unwrap()
                .collect_file(),
        )
    }
}
