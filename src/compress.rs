use super::message::StatusCode;

use bytes::BytesMut;

pub struct Compressor {
    before: usize,
    after: usize,
}

impl Compressor {
    pub fn new() -> Compressor {
        Compressor {
            before: 0,
            after: 0,
        }
    }

    pub fn get_stats(&self) -> (usize, usize) {
        (self.before, self.after)
    }

    pub fn reset_stats(&mut self) {
        self.before = 0;
        self.after = 0;
    }

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
    /// Accepts a mutable BytesMut and returns a view to a subslice from the same buffer or error code.
    pub fn compress(&mut self, mut buffer: BytesMut) -> Result<BytesMut, StatusCode> {
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
                end += Self::write_label(working, count, &mut buffer[end..]);
                working = current; // new working_char
                count = 1; // reset count
            }
        }

        end += Self::write_label(working, count, &mut buffer[end..]);

        // wait until end of valid buffer to update stats
        self.before += buffer.len();
        self.after += end;

        Ok(buffer.split_to(end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a() {
        let mut compressor = Compressor::new();
        assert_eq!(
            compressor.compress(BytesMut::from("a")),
            Ok(BytesMut::from("a"))
        );
    }

    #[test]
    fn aa() {
        let mut compressor = Compressor::new();
        assert_eq!(
            compressor.compress(BytesMut::from("aa")),
            Ok(BytesMut::from("aa"))
        );
    }

    #[test]
    fn aaa() {
        let mut compressor = Compressor::new();
        assert_eq!(
            compressor.compress(BytesMut::from("aaa")),
            Ok(BytesMut::from("3a"))
        );
    }

    #[test]
    fn aaaaabbb() {
        let mut compressor = Compressor::new();
        assert_eq!(
            compressor.compress(BytesMut::from("aaaaabbb")),
            Ok(BytesMut::from("5a3b"))
        );
    }

    #[test]
    fn aaaaabbbbbbaaabb() {
        let mut compressor = Compressor::new();
        assert_eq!(
            compressor.compress(BytesMut::from("aaaaabbbbbbaaabb")),
            Ok(BytesMut::from("5a6b3abb"))
        );
    }

    #[test]
    fn abcdefg() {
        let mut compressor = Compressor::new();
        assert_eq!(
            compressor.compress(BytesMut::from("abcdefg")),
            Ok(BytesMut::from("abcdefg"))
        );
    }

    #[test]
    fn aaaccddddhhhhi() {
        let mut compressor = Compressor::new();
        assert_eq!(
            compressor.compress(BytesMut::from("aaaccddddhhhhi")),
            Ok(BytesMut::from("3acc4d4hi"))
        );
    }

    #[test]
    fn _123() {
        let mut compressor = Compressor::new();
        assert_eq!(
            compressor.compress(BytesMut::from("123")),
            Err(StatusCode::NonAlphabetic)
        );
    }

    #[test]
    #[allow(non_snake_case)]
    fn abCD() {
        let mut compressor = Compressor::new();
        assert_eq!(
            compressor.compress(BytesMut::from("abCD")),
            Err(StatusCode::NonLowerCase)
        );
    }

    #[test]
    #[allow(non_snake_case)]
    fn u263A() {
        let mut compressor = Compressor::new();
        assert_eq!(
            compressor.compress(BytesMut::from("☺")),
            Err(StatusCode::NonAscii)
        );
    }

    #[test]
    fn empty() {
        let mut compressor = Compressor::new();
        assert_eq!(
            compressor.compress(BytesMut::new()),
            Err(StatusCode::EmptyBuffer)
        );
    }
}
