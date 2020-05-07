mod compress;
mod message;
mod packet;

use compress::Compressor;
use message::{RequestCode, StatusCode};
use packet::PacketCodec;

use bytes::{BufMut, BytesMut};
use futures::sink::SinkExt;
use std::sync::Arc;
use std::{error, io};
use tokio::net::TcpListener;
use tokio::stream::StreamExt;
use tokio::sync::Mutex;
use tokio_util::codec::Framed;

/// Global server stats.
struct Stats {
    received: usize,
    sent: usize,
    before: usize,
    after: usize,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn error::Error>> {
    let mut listener = TcpListener::bind(":::4000").await?;

    // create global stats
    let stats = Arc::new(Mutex::new(Stats {
        received: 0,
        sent: 0,
        before: 0,
        after: 0,
    }));

    loop {
        let (socket, _addr) = listener.accept().await?;
        let stats = stats.clone(); // local reference to global stats

        tokio::spawn(async move {
            // create packet codec with 16 KiB max payload length
            let mut stream = Framed::new(socket, PacketCodec::new_with_max_payload(1 << 14));
            let mut compressor = Compressor::new();

            loop {
                {
                    // get local stats
                    let (received, sent) = stream.codec().get_stats();
                    let (before, after) = compressor.get_stats();

                    // update global stats
                    let mut stats = stats.lock().await;
                    stats.received += received;
                    stats.sent += sent;
                    stats.before += before;
                    stats.after += after;

                    // reset local stats
                    stream.codec_mut().reset_stats();
                    compressor.reset_stats();
                } // <- drop stats lock here

                match stream.next().await {
                    // process request code
                    Some(Ok(request)) => {
                        match request {
                            RequestCode::Ping => {
                                stream.send(StatusCode::Ok(BytesMut::new())).await?;
                            }
                            RequestCode::GetStats => {
                                // should response be local stats instead of global stats?
                                let stats = stats.lock().await;
                                let mut buffer = BytesMut::with_capacity(9);

                                // don't forget to include this received packet
                                let (received, _) = stream.codec().get_stats();

                                // total packet bytes received and sent
                                buffer.put_u32((stats.received + received) as u32);
                                buffer.put_u32(stats.sent as u32); // big-endian order

                                // total payload bytes before and after compression
                                let percent = if stats.before == 0 {
                                    0.0 // or should the compression ratio be 100%?
                                } else {
                                    (stats.after as f32) / (stats.before as f32) * 100.0
                                };
                                buffer.put_u8(percent as u8);

                                // should the response bytes about to be sent be counted?
                                stream.send(StatusCode::Ok(buffer)).await?;
                            }
                            RequestCode::ResetStats => {
                                let mut stats = stats.lock().await;
                                stats.received = 0;
                                stats.sent = 0;
                                stats.before = 0;
                                stats.after = 0;
                                stream.codec_mut().reset_stats();
                                compressor.reset_stats();

                                // should the response bytes about to be sent be ignored?
                                stream.send(StatusCode::Ok(BytesMut::new())).await?;
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
