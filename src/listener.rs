use crate::Fetch;
use crate::{NamedRequest, Receive, Request, TorrentFile};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::io;
use tsyncp::channel::{channel_on, BincodeChannel};

pub struct PeerListener {
    files: Arc<Mutex<Vec<TorrentFile>>>,
    // listen_address: SocketAddr, // peers: Vec<SocketAddr> - should be session-wide only
}

impl PeerListener {
    pub fn listen(files: Arc<Mutex<Vec<TorrentFile>>>, listen_address: SocketAddr) {
        tokio::spawn(async move {
            let channel: BincodeChannel<NamedRequest> = channel_on(listen_address).await.unwrap();
            Self::process_stream(files, channel).await
        });
    }

    async fn process_stream(
        files: Arc<Mutex<Vec<TorrentFile>>>,
        mut channel: BincodeChannel<NamedRequest>,
    ) -> io::Result<()> {
        loop {
            let request = channel.recv().await.unwrap().unwrap();
            match request.request {
                Request::Client(_) => unreachable!(),
                Request::Listener(listener_request) => {
                    Self::process_listener(
                        files.clone(),
                        listener_request,
                        &mut channel,
                        request.name,
                    )
                    .await?
                }
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

    async fn process_listener(
        files: Arc<Mutex<Vec<TorrentFile>>>,
        request: Fetch,
        channel: &mut BincodeChannel<NamedRequest>,
        name: String,
    ) -> io::Result<()> {
        match request {
            Fetch::Numbers => {
                println!(
                    "Listener: Received request for segment numbers from {}. Sending...",
                    channel.peer_addr()
                );
                let numbers = files
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|file| file.name == name)
                    .map(|file| file.segments.iter().map(|segment| segment.index).collect());

                let request = NamedRequest::new(name, Receive::Numbers(numbers).into());

                channel.send(request).await.unwrap();
            }
            Fetch::SegmentNumber(seg_number) => {
                println!("Listener: Received request for segment number {seg_number} from {}. Sending...", channel.peer_addr());
                let segment = {
                    let guard = files.lock().unwrap();

                    guard
                        .iter()
                        .find(|file| file.name == name)
                        .and_then(|file| {
                            file.segments
                                .iter()
                                .find(|segment| segment.index == seg_number)
                        })
                        .cloned()
                };

                let request = NamedRequest::new(name, Receive::Segment(segment.unwrap()).into());

                channel.send(request).await.unwrap();
            }
            Fetch::FileInfo => {
                let maybe_size = files
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|file| file.name == name)
                    .map(|file| file.intended_size); // can't do if let because the future won't be safe

                if let Some(size) = maybe_size {
                    println!("Listener: Request from {}. File {} with size {} found. Sending file info...", channel.peer_addr(), name, size);
                    let request = NamedRequest::new(name, Receive::FileSize(size).into());
                    channel.send(request).await.unwrap();
                }
            }
        }
        Ok(())
    }
}
