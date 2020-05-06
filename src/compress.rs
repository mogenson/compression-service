#[path = "message.rs"]
mod message;
use message::StatusCode;

/// Writes the number of repeated letters, then letter, or original letters
/// to slice, whichever sequence is shorter. Returns the number of letters written.
fn write_label(letter: char, count: usize, buffer: &mut [u8]) -> usize {
    let label = count.to_string();
    let length = label.len() + 1;
    if length < count {
        for (i, digit) in label.bytes().enumerate() {
            buffer[i] = digit; // write label
        }

        buffer[label.len()] = letter as u8; // then write letter

        length
    } else {
        // We could check if we're at the begining of the buffer to avoid
        // an unnecessary overwrite of the same letters. But this is a rare case.
        buffer.iter_mut().take(count).for_each(|x| {
            *x = letter as u8; // write original letters back
        });

        count
    }
}

/// Compresses a buffer using a simplified prefix encoding compression scheme.
///
/// Accepts a mutable reference to a buffer and returns a reference to a
/// compressed subslice from the same buffer or error code.
pub fn compress_inline(buffer: &mut [u8]) -> Result<&[u8], StatusCode> {
    if buffer.is_empty() {
        return Err(StatusCode::EmptyBuffer);
    }

    // init state
    let mut working = buffer[0] as char;
    let mut count = 0;
    let mut end = 0;

    for i in 0..buffer.len() {
        let current = buffer[i] as char;

        // input check
        if !current.is_ascii() {
            return Err(StatusCode::NonAscii);
        }
        if !current.is_ascii_alphabetic() {
            return Err(StatusCode::NonAlphabetic);
        }
        if !current.is_ascii_lowercase() {
            return Err(StatusCode::NonLowerCase);
        }

        if current == working {
            count += 1; // increment count and continue
        } else {
            end += write_label(working, count, &mut buffer[end..]);
            working = current; // new working_char
            count = 1; // reset count
        }
    }

    end += write_label(working, count, &mut buffer[end..]);

    Ok(&buffer[0..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    // For these tests, we need a mutable String that lives long enough to pass
    // a mutable slice to compress_inline(). The function as_bytes_mut()
    // is unsafe, but that does not affect the stability of compress_inline().

    #[test]
    fn a() {
        let mut input = String::from("a");
        let mut buffer = unsafe { input.as_bytes_mut() };
        assert_eq!(compress_inline(&mut buffer), Ok("a".as_bytes()));
    }

    #[test]
    fn aa() {
        let mut input = String::from("aa");
        let mut buffer = unsafe { input.as_bytes_mut() };
        assert_eq!(compress_inline(&mut buffer), Ok("aa".as_bytes()));
    }

    #[test]
    fn aaa() {
        let mut input = String::from("aaa");
        let mut buffer = unsafe { input.as_bytes_mut() };
        assert_eq!(compress_inline(&mut buffer), Ok("3a".as_bytes()));
    }

    #[test]
    fn aaaaabbb() {
        let mut input = String::from("aaaaabbb");
        let mut buffer = unsafe { input.as_bytes_mut() };
        assert_eq!(compress_inline(&mut buffer), Ok("5a3b".as_bytes()));
    }

    #[test]
    fn aaaaabbbbbbaaabb() {
        let mut input = String::from("aaaaabbbbbbaaabb");
        let mut buffer = unsafe { input.as_bytes_mut() };
        assert_eq!(compress_inline(&mut buffer), Ok("5a6b3abb".as_bytes()));
    }

    #[test]
    fn abcdefg() {
        let mut input = String::from("abcdefg");
        let mut buffer = unsafe { input.as_bytes_mut() };
        assert_eq!(compress_inline(&mut buffer), Ok("abcdefg".as_bytes()));
    }

    #[test]
    fn aaaccddddhhhhi() {
        let mut input = String::from("aaaccddddhhhhi");
        let mut buffer = unsafe { input.as_bytes_mut() };
        assert_eq!(compress_inline(&mut buffer), Ok("3acc4d4hi".as_bytes()));
    }

    #[test]
    fn _123() {
        let mut input = String::from("123");
        let mut buffer = unsafe { input.as_bytes_mut() };
        assert_eq!(compress_inline(&mut buffer), Err(StatusCode::NonAlphabetic));
    }

    #[test]
    #[allow(non_snake_case)]
    fn abCD() {
        let mut input = String::from("abCD");
        let mut buffer = unsafe { input.as_bytes_mut() };
        assert_eq!(compress_inline(&mut buffer), Err(StatusCode::NonLowerCase));
    }

    #[test]
    #[allow(non_snake_case)]
    fn u263A() {
        let mut input = String::from("☺");
        let mut buffer = unsafe { input.as_bytes_mut() };
        assert_eq!(compress_inline(&mut buffer), Err(StatusCode::NonAscii));
    }

    #[test]
    fn empty() {
        assert_eq!(
            compress_inline(&mut [] as &mut [u8]),
            Err(StatusCode::EmptyBuffer)
        );
    }
}
