use crate::{NamedRequest, OldRequest, TorrentFile};
use std::fmt::Debug;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::io;
use tokio::io::AsyncReadExt;
use tsyncp::channel::{channel_on, BincodeChannel};

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

        let peer = PeerListener {
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
                    let numbers = self.find_file(&request.name).and_then(|file| {
                        Some(file.segments.iter().map(|segment| segment.index).collect())
                    });

                    let request =
                        NamedRequest::new(request.name, OldRequest::ReceiveNumbers { numbers });

                    channel.send(request).await.unwrap();
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

                    channel.send(request).await.unwrap();
                }
                OldRequest::ReceiveNumbers { numbers } => match numbers {
                    None => {}
                    Some(numbers) => {
                        for seg_number in numbers {
                            let request = NamedRequest::new(
                                request.name.clone(),
                                OldRequest::FetchSegment { seg_number },
                            );
                            channel.send(request).await.unwrap();
                        }
                    }
                },
                OldRequest::ReceiveSegment { segment } => {
                    self.files
                        .iter_mut()
                        .find(|file| file.name == request.name)
                        .and_then(|file| Some(file.add_segment(segment)));

                    let file = self.find_file(&request.name).unwrap();
                    if file.is_complete() {
                        channel
                            .send(NamedRequest::new(request.name, OldRequest::Finished))
                            .await
                            .unwrap();
                    }
                }
                OldRequest::Finished => {
                    channel
                        .send(NamedRequest::new(request.name, OldRequest::Finished))
                        .await
                        .unwrap();
                    break;
                }
                OldRequest::FetchFileInfo => match self.find_file(&request.name) {
                    None => {}
                    Some(file) => {
                        let request = NamedRequest::new(
                            request.name,
                            OldRequest::ReceiveFileInfo { size: file.size },
                        );
                        channel.send(request).await.unwrap();
                    }
                },
                OldRequest::ReceiveFileInfo { size } => {
                    self.create_empty_file(&request.name, size);
                    let request = NamedRequest::new(request.name, OldRequest::FetchNumbers);
                    channel.send(request).await.unwrap();
                }
            }
        }
        Ok(())
    }
}
