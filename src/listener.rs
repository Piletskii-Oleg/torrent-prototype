use crate::storage::Storage;
use crate::Fetch;
use crate::{NamedRequest, Receive, Request};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::io;
use tsyncp::channel::{channel_on, BincodeChannel};

pub struct PeerListener;

impl PeerListener {
    pub fn listen(files: Arc<Mutex<dyn Storage + Send + Sync>>, listen_address: SocketAddr) {
        tokio::spawn(async move {
            let channel: BincodeChannel<NamedRequest> = channel_on(listen_address).await.unwrap();
            Self::process_stream(files, channel).await
        });
    }

    async fn process_stream(
        files: Arc<Mutex<dyn Storage + Send + Sync>>,
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
        files: Arc<Mutex<dyn Storage + Send + Sync>>,
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
                let numbers = files.lock().unwrap().segment_numbers(&name);

                let request = NamedRequest::new(name, Receive::Numbers(numbers).into());

                channel.send(request).await.unwrap();
            }
            Fetch::SegmentNumber(seg_number) => {
                println!("Listener: Received request for segment number {seg_number} from {}. Sending...", channel.peer_addr());
                let segment = {
                    let guard = files.lock().unwrap();
                    guard.segment(&name, seg_number)
                };

                let request = NamedRequest::new(name, Receive::Segment(segment).into());

                channel.send(request).await.unwrap();
            }
            Fetch::FileInfo => {
                let maybe_size = files.lock().unwrap().file_size(&name); // can't do if let because the future won't be safe

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
