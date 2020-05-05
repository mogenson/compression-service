mod compress;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    //let mut buffer = ['a' as u8, 'b' as u8, 'c' as u8];
    let mut string = String::from("hello");
    let mut buffer = unsafe { string.as_bytes_mut() };
    match compress::compress_inline(&mut buffer) {
        Ok(s) => println!("ok: {}", std::str::from_utf8(s).unwrap()),
        Err(e) => println!("error: {}", e),
    }

    Ok(())
}
