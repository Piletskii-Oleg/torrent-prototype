use std::error::Error;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let socket = SocketAddr::new("127.0.0.1".parse().unwrap(), 8000);
    let mut stream = TcpStream::connect(socket).await?;
    let (mut reader, mut writer) = stream.split();

    let text = b"text";
    writer.write_all(text).await?;

    let mut buf = vec![];
    reader.read_to_end(&mut buf).await?;

    println!("{}", String::from_utf8(buf.clone()).unwrap());
    assert_eq!(buf, text.to_ascii_uppercase());
    Ok(())
}
