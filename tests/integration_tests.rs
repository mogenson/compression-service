use bytes::{BufMut, BytesMut};
use std::error::Error;
use std::io::prelude::*;
use std::net::TcpStream;
use std::process::Command;
use std::thread;
use std::time::Duration;

fn transceive_packet(
    stream: &mut TcpStream,
    request: u16,
    payload: &[u8],
    response: &mut [u8],
) -> Result<(), Box<dyn Error>> {
    let mut packet = BytesMut::with_capacity(8 + payload.len());
    packet.put("STRY".as_bytes());
    packet.put_u16(payload.len() as u16);
    packet.put_u16(request);
    packet.put(payload);
    stream.write_all(&packet[..])?;
    stream.read_exact(response)?;
    Ok(())
}

#[test]
fn integration_tests() -> Result<(), Box<dyn Error>> {
    // use only one integration test so that we can run the following
    // sequentially, keep the server alive, and generate some stats
    let mut server = Command::new("cargo").arg("run").spawn()?;
    thread::sleep(Duration::from_secs(1)); // wait for server to start
    let mut stream = TcpStream::connect("::1:4000")?;

    // test good packets

    // ping
    let mut response = [0; 8];
    transceive_packet(&mut stream, 1, &[], &mut response)?;
    assert_eq!(&response, b"STRY\0\0\0\0", "ping failed");

    // compress "a"
    let mut response = [0; 9];
    transceive_packet(&mut stream, 4, "a".as_bytes(), &mut response)?;
    assert_eq!(&response, b"STRY\0\x01\0\0a", "compress 'a' failed");

    // compress "aa"
    let mut response = [0; 10];
    transceive_packet(&mut stream, 4, "aa".as_bytes(), &mut response)?;
    assert_eq!(&response, b"STRY\0\x02\0\0aa", "compress 'aa' failed");

    // compress "aaa"
    let mut response = [0; 10];
    transceive_packet(&mut stream, 4, "aaa".as_bytes(), &mut response)?;
    assert_eq!(&response, b"STRY\0\x02\0\03a", "compress 'aaa' failed");

    // compress "aaaaabbb"
    let mut response = [0; 12];
    transceive_packet(&mut stream, 4, "aaaaabbb".as_bytes(), &mut response)?;
    assert_eq!(
        &response, b"STRY\0\x04\0\05a3b",
        "compress 'aaaaabbb' failed"
    );

    // compress "aaaaabbbbbbaaabb"
    let mut response = [0; 16];
    transceive_packet(&mut stream, 4, "aaaaabbbbbbaaabb".as_bytes(), &mut response)?;
    assert_eq!(
        &response, b"STRY\0\x08\0\05a6b3abb",
        "compress 'aaaaabbbbbbaaabb' failed"
    );

    // compress "abcdefg"
    let mut response = [0; 15];
    transceive_packet(&mut stream, 4, "abcdefg".as_bytes(), &mut response)?;
    assert_eq!(
        &response, b"STRY\0\x07\0\0abcdefg",
        "compress 'abcdefg' failed"
    );

    // compress "aaaccddddhhhhi"
    let mut response = [0; 17];
    transceive_packet(&mut stream, 4, "aaaccddddhhhhi".as_bytes(), &mut response)?;
    assert_eq!(
        &response, b"STRY\0\x09\0\03acc4d4hi",
        "compress 'aaaccddddhhhhi' failed"
    );

    // split packet
    let mut response = [0; 19];
    stream.write_all("STRY\0".as_bytes())?;
    thread::sleep(Duration::from_millis(1));
    stream.write_all("\x0c\0".as_bytes())?;
    thread::sleep(Duration::from_millis(10));
    stream.write_all("\x04cross".as_bytes())?;
    thread::sleep(Duration::from_millis(100));
    stream.write_all("section".as_bytes())?;
    stream.read_exact(&mut response)?;
    assert_eq!(
        &response, b"STRY\0\x0b\0\0cro3section",
        "compress 'crosssection' failed"
    );

    // get stats
    let mut response = [0; 17];
    transceive_packet(&mut stream, 2, &[], &mut response)?;
    assert_eq!(
        &response, b"STRY\0\x09\0\0\0\0\0\x8f\0\0\0\x74\x45",
        "get stats failed"
    );

    // reset stats
    let mut response = [0; 8];
    transceive_packet(&mut stream, 3, &[], &mut response)?;
    assert_eq!(&response, b"STRY\0\0\0\0", "reset stats failed");

    // check get stats again
    let mut response = [0; 17];
    transceive_packet(&mut stream, 2, &[], &mut response)?;
    assert_eq!(
        &response, b"STRY\0\x09\0\0\0\0\0\x08\0\0\0\x08\0",
        "get stats failed"
    );

    // test bad packets

    // compress without payload
    let mut response = [0; 8];
    transceive_packet(&mut stream, 4, &[], &mut response)?;
    assert_eq!(
        &response, b"STRY\0\0\0\x21",
        "compress without payload did not return EmptyBufer error"
    );

    // ping with payload
    let mut response = [0; 8];
    transceive_packet(&mut stream, 1, "hello".as_bytes(), &mut response)?;
    assert_eq!(
        &response, b"STRY\0\0\0\x22",
        "ping with payload did not return NonEmptyBuffer error"
    );

    // compress "☺"
    let mut response = [0; 8];
    transceive_packet(&mut stream, 4, "☺".as_bytes(), &mut response)?;
    assert_eq!(
        &response, b"STRY\0\0\0\x23",
        "compress '☺' did not return NonAscii error"
    );

    // compress "123"
    let mut response = [0; 8];
    transceive_packet(&mut stream, 4, "123".as_bytes(), &mut response)?;
    assert_eq!(
        &response, b"STRY\0\0\0\x24",
        "compress '123' did not return NonAlphabetic error"
    );

    // compress "abCD"
    let mut response = [0; 8];
    transceive_packet(&mut stream, 4, "abCD".as_bytes(), &mut response)?;
    assert_eq!(
        &response, b"STRY\0\0\0\x25",
        "compress 'abCD' did not return NonLowerCase error"
    );

    // compress "X Æ A-12"
    let mut response = [0; 8];
    transceive_packet(&mut stream, 4, "X Æ A-12".as_bytes(), &mut response)?;
    assert_eq!(
        &response, b"STRY\0\0\0\x25",
        "I think it's pronounced 'Kyle'"
    );

    server.kill()?;
    Ok(())
}
