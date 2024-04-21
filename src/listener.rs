use crate::{NamedRequest, OldRequest, TorrentFile};
use std::fmt::Debug;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::io;
use tokio::io::AsyncReadExt;
use tsyncp::channel::{channel_on, BincodeChannel};

pub struct PeerListener {
    files: Arc<Mutex<Vec<TorrentFile>>>,
    listen_address: SocketAddr, // peers: Vec<SocketAddr> - should be session-wide only
}

impl PeerListener {
    pub fn new_listen(files: Arc<Mutex<Vec<TorrentFile>>>, listen_address: SocketAddr) {
        let peer = PeerListener {
            files,
            listen_address,
        };

        tokio::spawn(async move { peer.listen(listen_address).await });
    }

    fn create_empty_file(&mut self, name: &str, size: usize) {
        self.files.lock().unwrap().push(TorrentFile {
            segments: vec![],
            name: name.to_string(),
            size,
        })
    }

    async fn listen(mut self, addr: SocketAddr) -> io::Result<()> {
        let channel: BincodeChannel<NamedRequest> = channel_on(addr).await.unwrap();
        self.process_stream(channel).await
    }

    async fn process_stream(
        &mut self,
        mut channel: BincodeChannel<NamedRequest>,
    ) -> io::Result<()> {
        loop {
            let request = channel.recv().await.unwrap().unwrap();

            match request.request {
                OldRequest::FetchNumbers => {
                    println!(
                        "Listener: Received request for segment numbers from {}. Sending...",
                        channel.peer_addr()
                    );
                    let numbers = self
                        .files
                        .lock()
                        .unwrap()
                        .iter()
                        .find(|file| file.name == request.name)
                        .and_then(|file| {
                            Some(file.segments.iter().map(|segment| segment.index).collect())
                        });

                    let request =
                        NamedRequest::new(request.name, OldRequest::ReceiveNumbers { numbers });

                    channel.send(request).await.unwrap();
                }
                OldRequest::FetchSegment { seg_number } => {
                    println!("Listener: Received request for segment number {seg_number} from {}. Sending...", channel.peer_addr());
                    let segment = {
                        let guard = self.files.lock().unwrap();

                        guard
                            .iter()
                            .find(|file| file.name == request.name)
                            .and_then(|file| {
                                file.segments
                                    .iter()
                                    .find(|segment| segment.index == seg_number)
                            })
                            .map(|segment| segment.clone())
                    };

                    let request = NamedRequest::new(
                        request.name,
                        OldRequest::ReceiveSegment {
                            segment: segment.unwrap(),
                        },
                    );

                    channel.send(request).await.unwrap();
                }
                OldRequest::Finished => {
                    println!("{} transfer complete.", request.name);
                    channel
                        .send(NamedRequest::new(request.name, OldRequest::Finished))
                        .await
                        .unwrap();
                    break;
                }
                OldRequest::FetchFileInfo => {
                    let maybe_size = {
                        self.files
                            .lock()
                            .unwrap()
                            .iter()
                            .find(|file| file.name == request.name)
                            .map(|file| file.size)
                    };
                    if let Some(size) = maybe_size {
                        println!("Listener: Request from {}. File {} with size {} found. Sending file info...", channel.peer_addr(), request.name, size);
                        let request = NamedRequest::new(
                            request.name,
                            OldRequest::ReceiveFileInfo { size },
                        );
                        channel.send(request).await.unwrap();
                    }
                }
                _ => unreachable!(),
            }
        }
        Ok(())
    }
}
