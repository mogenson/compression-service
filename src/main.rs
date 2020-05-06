mod compress;
mod packet;

use packet::PacketCodec;

use std::error::Error;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::prelude::*;
use tokio::stream::StreamExt;
use tokio_util::codec::Framed;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut listener = TcpListener::bind("localhost:4000").await?;

    loop {
        let (mut socket, _) = listener.accept().await?;
        tokio::spawn(async move {
            // create packet codec with 16 KiB max payload length
            let mut stream = Framed::new(socket, PacketCodec::new_with_max_payload(1 << 14));

            loop {
                match stream.next().await {
                    Some(Ok(request)) => println!("request: {:?}", request),
                    Some(Err(e)) => eprintln!("error: {:?}", e),
                    None => {}
                }

                // let n = match socket.read(&mut buf).await {
                //     Ok(n) if n == 0 => return,
                //     Ok(n) => n,
                //     Err(e) => {
                //         eprintln!("read error: {:?}", e);
                //         return;
                //     }
                // };

                // if let Err(e) = socket.write_all(&buf[0..n]).await {
                //     eprintln!("write error: {:?}", e);
                //     return;
                // }
            }
        });
    }
}
