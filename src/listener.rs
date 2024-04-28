use crate::ListenerRequest;
use crate::{ClientRequest, NamedRequest, Request, TorrentFile};
use std::fmt::Debug;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::io;
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
                Request::ClientRequest(_) => unreachable!(),
                Request::ListenerRequest(listener_request) => self.process_listener(listener_request, &mut channel, request.name).await?,
                Request::Finished => {
                    println!("{} transfer complete.", request.name);
                    channel
                        .send(NamedRequest::new(request.name, Request::Finished))
                        .await
                        .unwrap();
                    break;
                }
            };
        }
        Ok(())
    }

    async fn process_listener(&mut self, request: ListenerRequest, channel: &mut BincodeChannel<NamedRequest>, name: String) -> io::Result<()> {
        match request {
            ListenerRequest::FetchNumbers => {
                println!(
                    "Listener: Received request for segment numbers from {}. Sending...",
                    channel.peer_addr()
                );
                let numbers = self
                    .files
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|file| file.name == name)
                    .and_then(|file| {
                        Some(file.segments.iter().map(|segment| segment.index).collect())
                    });

                let request =
                    NamedRequest::new(name, ClientRequest::ReceiveNumbers { numbers }.into());

                channel.send(request).await.unwrap();
            }
            ListenerRequest::FetchSegment { seg_number } => {
                println!("Listener: Received request for segment number {seg_number} from {}. Sending...", channel.peer_addr());
                let segment = {
                    let guard = self.files.lock().unwrap();

                    guard
                        .iter()
                        .find(|file| file.name == name)
                        .and_then(|file| {
                            file.segments
                                .iter()
                                .find(|segment| segment.index == seg_number)
                        })
                        .map(|segment| segment.clone())
                };

                let request = NamedRequest::new(
                    name,
                    ClientRequest::ReceiveSegment {
                        segment: segment.unwrap(),
                    }.into(),
                );

                channel.send(request).await.unwrap();
            }
            ListenerRequest::FetchFileInfo => {
                let maybe_size = {
                    self.files
                        .lock()
                        .unwrap()
                        .iter()
                        .find(|file| file.name == name)
                        .map(|file| file.size)
                };
                if let Some(size) = maybe_size {
                    println!("Listener: Request from {}. File {} with size {} found. Sending file info...", channel.peer_addr(), name, size);
                    let request =
                        NamedRequest::new(name, ClientRequest::ReceiveFileInfo { size }.into());
                    channel.send(request).await.unwrap();
                }
            }
        }
        Ok(())
    }
}


