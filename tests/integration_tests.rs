use std::error::Error;
use std::io::prelude::*;
use std::net::TcpStream;
use std::process::Command;

#[test]
fn ping() -> Result<(), Box<dyn Error>> {
    let mut server = Command::new("cargo").arg("run").spawn()?;

    let query = [83, 84, 82, 89, 0, 0, 0, 1];
    let mut response = [0; 8];

    let mut stream = TcpStream::connect("::1:4000")?;
    stream.write_all(&query)?;
    stream.read_exact(&mut response)?;

    server.kill()?;

    assert_eq!(query, response);

    Ok(())
}
