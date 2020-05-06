mod compress;
mod message;
mod packet;

use compress::Compressor;
use message::{RequestCode, StatusCode};
use packet::PacketCodec;

use bytes::{BufMut, BytesMut};
use futures::sink::SinkExt;
use std::{error, io};
use tokio::net::TcpListener;
use tokio::stream::StreamExt;
use tokio_util::codec::Framed;

#[tokio::main]
async fn main() -> Result<(), Box<dyn error::Error>> {
    let mut listener = TcpListener::bind(":::4000").await?;

    loop {
        let (socket, _addr) = listener.accept().await?;

        tokio::spawn(async move {
            // Note: PacketCodec and Compressor are client specific which means stats will
            // be unique per client. If global stats are desired, we'll need to iterate over
            // spawned tasks to collect data when a request is received, or periodically.
            let codec = PacketCodec::new_with_max_payload(1 << 14); // 16 KiB max payload length
            let mut stream = Framed::new(socket, codec);
            let mut compressor = Compressor::new();

            loop {
                match stream.next().await {
                    // process request code
                    Some(Ok(request)) => {
                        match request {
                            RequestCode::Ping => {
                                stream.send(StatusCode::Ok(BytesMut::new())).await?
                            }
                            RequestCode::GetStats => {
                                // should the response bytes about to be sent be counted?
                                let mut stats = BytesMut::with_capacity(9);

                                // total packet bytes received and sent
                                let (received, sent) = stream.codec().get_stats();
                                stats.put_u32(received as u32); // big-endian order
                                stats.put_u32(sent as u32); // big-endian order

                                // total payload bytes before and after compression
                                let (before, after) = compressor.get_stats();
                                let percent = if before == 0 {
                                    0.0 // or should the compression ratio be 100%?
                                } else {
                                    (after as f32) / (before as f32) * 100.0
                                };
                                stats.put_u8(percent as u8);

                                stream.send(StatusCode::Ok(stats)).await?
                            }
                            RequestCode::ResetStats => {
                                // should the response bytes about to be sent be ignored?
                                stream.codec_mut().reset_stats();
                                compressor.reset_stats();
                                stream.send(StatusCode::Ok(BytesMut::new())).await?
                            }
                            RequestCode::Compress(payload) => match compressor.compress(payload) {
                                Ok(compressed) => stream.send(StatusCode::Ok(compressed)).await?,
                                Err(error) => stream.send(error).await?,
                            },
                        };
                    }

                    // pass parsing errors back to encoder to be sent as status code packets
                    Some(Err(error)) => stream.send(error).await?,
                    // stream has closed, exit loop
                    None => break,
                }
            }

            Ok::<(), io::Error>(()) // <- https://bit.ly/2SHCI4a
        });
    }
}
