use crate::{NamedRequest, Request, TorrentFile};
use std::error::Error;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tsyncp::channel::{BincodeChannel, channel_to};

pub(crate) struct PeerClient {
    pub(super) files: Vec<TorrentFile>,
    //tracker: SocketAddr,
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

        PeerClient { files }
    }

    pub(super) async fn download_file_peer(
        &mut self,
        name: String,
        socket: SocketAddr,
    ) -> Result<(), Box<dyn Error>> {
        // let mut rx: BincodeReceiver<NamedRequest> = receiver_on(socket).await?;
        {
          //  let stream = TcpStream::connect(socket).await?;
        }
        let mut channel: BincodeChannel<NamedRequest> = channel_to(socket).retry(Duration::from_secs(1), 5).await?;

        let fetch_numbers = NamedRequest {
            name: name.to_string(),
            request: Request::FetchFileInfo,
        };

        channel.send(fetch_numbers).await?;

        self.process_stream(&mut channel).await?;

        Ok(())
    }

    pub async fn download_file(&mut self, name: String) -> io::Result<()> {
        let peer_sockets = Self::fetch_peers().await?; // self.tracker
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

    async fn fetch_peers() -> io::Result<Vec<SocketAddr>> { // tracker: SocketAddr
        unimplemented!()
    }

    pub(super) async fn process_stream(&mut self, mut channel: &mut BincodeChannel<NamedRequest>) -> Result<(), Box<dyn Error>> {
        loop {
            let request = channel.recv().await.unwrap()?;

            match request.request {
                Request::FetchNumbers => {
                    let numbers = self.find_file(&request.name).and_then(|file| {
                        Some(file.segments.iter().map(|segment| segment.index).collect())
                    });

                    let request =
                        NamedRequest::new(request.name, Request::ReceiveNumbers { numbers });

                    channel.send(request).await?;
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

                    channel.send(request).await?;
                }
                Request::ReceiveNumbers { numbers } => match numbers {
                    None => {}
                    Some(numbers) => {
                        for seg_number in numbers {
                            let request = NamedRequest::new(request.name.clone(), Request::FetchSegment { seg_number });
                            channel.send(request).await?;
                        }
                    },
                }
                Request::ReceiveSegment { segment } => {
                    let a = self
                        .files
                        .iter_mut()
                        .find(|file| file.name == request.name)
                        .and_then(|file| Some(file.add_segment(segment)));
                    println!("{a:?}");

                    let file = self.find_file(&request.name).unwrap();
                    if file.is_complete() {
                        channel.send(NamedRequest::new(request.name, Request::Finished)).await?;
                    }
                }
                Request::Finished => {
                    channel.send(NamedRequest::new(request.name, Request::Finished)).await?;
                    break;
                }
                Request::FetchFileInfo => match self.find_file(&request.name) {
                    None => {}
                    Some(file) => {
                        let request = NamedRequest::new(
                            request.name,
                            Request::ReceiveFileInfo { size: file.size },
                        );
                        channel.send(request).await?;
                    }
                },
                Request::ReceiveFileInfo { size } => {
                    self.create_empty_file(&request.name, size);
                    let request = NamedRequest::new(request.name, Request::FetchNumbers);
                    channel.send(request).await?;
                }
            }
        }
        println!("downloaded");
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
