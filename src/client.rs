use crate::{NamedRequest, Request, TorrentFile};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub(crate) struct PeerClient {
    pub(super) files: Vec<TorrentFile>,
    tracker: SocketAddr,
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

        PeerClient { files, tracker }
    }

    pub(super) async fn download_file_peer(
        &mut self,
        name: String,
        socket: SocketAddr,
    ) -> io::Result<()> {
        let mut stream = TcpStream::connect(socket).await?;

        let (mut reader, mut writer) = stream.split();

        let fetch_numbers = bincode::serialize(&NamedRequest {
            name: name.to_string(),
            request: Request::FetchFileInfo,
        })
        .unwrap(); // unwrap?

        writer.write_all(&fetch_numbers).await?;



        Ok(())
    }

    pub async fn download_file(&mut self, name: String) -> io::Result<()> {
        let peer_sockets = Self::fetch_peers(self.tracker).await?;
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

    async fn fetch_peers(tracker: SocketAddr) -> io::Result<Vec<SocketAddr>> {
        unimplemented!()
    }
}
