use std::error::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut stream = TcpStream::connect("127.0.0.1:8000").await?;
    let (mut reader, mut writer) = stream.split();

    let text = b"text";
    writer.write_all(text).await?;

    let mut buf = vec![];
    reader.read_to_end(&mut buf).await?;

    assert_eq!(buf, text.to_ascii_uppercase());
    Ok(())
}
