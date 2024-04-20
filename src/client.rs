use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::io;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use crate::{NamedRequest, Request, TorrentFile};

pub(crate) struct PeerClient {
    pub(super) files: Vec<TorrentFile>,
    tracker: SocketAddr
}

impl PeerClient {
    pub fn new(folder: PathBuf, tracker: SocketAddr) -> Self {
        let file_paths = std::fs::read_dir(folder)
            .unwrap()
            .map(|dir| dir.unwrap().path())
            .filter(|dir| dir.is_file())
            .collect::<Vec<_>>();

        let files = file_paths.into_iter().map(|path| {
            let data = std::fs::read(&path).unwrap();
            let name = path.file_name().unwrap().to_str().unwrap();
            TorrentFile::new(data, name)
        }).collect();

        PeerClient {
            files,
            tracker,
        }
    }

    pub(super) async fn download_file_peer(&mut self, name: &str, socket: SocketAddr) -> io::Result<()> {
        println!("connecting to {socket}");
        let mut stream = TcpStream::connect(socket).await?;

        let (mut reader, mut writer) = stream.split();

        let fetch_numbers = bincode::serialize(&NamedRequest {
            name: name.to_string(),
            request: Request::FetchNumbers,
        })
            .unwrap(); // unwrap?

        writer.write_all(&fetch_numbers).await?;

        Ok(())
    }

    pub async fn download_file(&mut self, name: &str) -> io::Result<()> {
        let peer_sockets = Self::fetch_peers(self.tracker).await?;
        for socket in peer_sockets {
            let mut stream = TcpStream::connect(socket).await?;
            let (mut reader, mut writer) = stream.split();

            let fetch_numbers = bincode::serialize(&NamedRequest {
                name: name.to_string(),
                request: Request::FetchNumbers,
            })
                .unwrap(); // unwrap?

            writer.write_all(&fetch_numbers).await?;
        }
        Ok(())
    }

    async fn fetch_peers(tracker: SocketAddr) -> io::Result<Vec<SocketAddr>> {
        unimplemented!()
    }
}
