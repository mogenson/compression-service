#[path = "message.rs"]
mod message;
use message::{RequestCode, StatusCode};

use bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

pub struct PacketCodec {
    max_payload_len: usize,
    state: DecodeState,
}

impl PacketCodec {
    const MAGIC_HEADER: &'static str = "STRY"; // 0x53545259

    pub fn new_with_max_payload(max_payload: usize) -> PacketCodec {
        // Note: if max_payload was a run time user provided value instead of a
        // compile time constant, we should return a Result instead of panicking
        assert!(
            max_payload >= (1 << 12),
            "max payload less than 4 KiB limit"
        );
        assert!(
            max_payload < (1 << 15),
            "max payload greater or equal to 32 KiB limit"
        );

        PacketCodec {
            max_payload_len: max_payload,
            state: DecodeState::MagicHeader,
        }
    }
}

enum DecodeState {
    MagicHeader,
    PayloadLen, // pass payload length from PayloadLen through RequestCode to Payload
    RequestCode { length: usize },
    Payload { length: usize },
}

impl Decoder for PacketCodec {
    type Item = RequestCode;
    type Error = StatusCode;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match self.state {
            DecodeState::MagicHeader => {
                if src.len() < PacketCodec::MAGIC_HEADER.len() {
                    return Ok(None); // keep reading
                }

                // check for magic header, don't advance src yet
                let index = if &src[0..PacketCodec::MAGIC_HEADER.len()]
                    == PacketCodec::MAGIC_HEADER.as_bytes()
                {
                    self.state = DecodeState::PayloadLen; // move on to parsing payload length
                    PacketCodec::MAGIC_HEADER.len()
                } else {
                    1
                };

                // either advance src to payload length section
                // or advance by 1 byte and look for magic header again
                src.advance(index);
                self.decode(src) // recursively keep parsing
            }
            DecodeState::PayloadLen => {
                if src.len() < 2 {
                    return Ok(None);
                }

                let length = src.get_u16() as usize; // uses big-endian order
                if length > self.max_payload_len {
                    self.state = DecodeState::MagicHeader; // reset parsing
                    Err(StatusCode::MessageTooLarge)
                } else {
                    self.state = DecodeState::RequestCode { length };
                    self.decode(src) // recursively keep parsing
                }
            }
            DecodeState::RequestCode { length } => {
                if src.len() < 2 {
                    return Ok(None); // keep reading
                }

                self.state = DecodeState::MagicHeader; // reset in case pasrsing is done
                match src.get_u16() {
                    // Note: should we read payloads for packets with a non-zero payload
                    // length field but a request code that shouldn't have a payload?
                    1 => {
                        if length == 0 {
                            Ok(Some(RequestCode::Ping))
                        } else {
                            Err(StatusCode::NonEmptyBuffer)
                        }
                    }
                    2 => {
                        if length == 0 {
                            Ok(Some(RequestCode::GetStats))
                        } else {
                            Err(StatusCode::NonEmptyBuffer)
                        }
                    }
                    3 => {
                        if length == 0 {
                            Ok(Some(RequestCode::ResetStats))
                        } else {
                            Err(StatusCode::NonEmptyBuffer)
                        }
                    }
                    4 => {
                        if length == 0 {
                            // a compress request without a payload is invalid
                            Err(StatusCode::EmptyBuffer)
                        } else {
                            self.state = DecodeState::Payload { length };
                            src.reserve(length); // allocate space for payload
                            self.decode(src) // recursively keep parsing
                        }
                    }
                    _ => Err(StatusCode::UnsupportedRequestType),
                }
            }
            DecodeState::Payload { length } => {
                if src.len() < length {
                    // Note: should we have a timeout in case the full payload never arrives?
                    return Ok(None); // keep reading
                }

                // Idea: if it's ok to mix decoding and compressing state, we could
                // compress payload chunks as they arrive for faster performance
                let payload = src.split_to(length);
                self.state = DecodeState::MagicHeader; // reset for next packet

                // Compress is the only RequestCode with a payload
                Ok(Some(RequestCode::Compress(payload)))
            }
        }
    }
}

impl Encoder for PacketCodec {
    type Item = StatusCode;
    type Error = StatusCode;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // write magic header
        dst.reserve(PacketCodec::MAGIC_HEADER.len() + 4); // make space for header
        dst.put(PacketCodec::MAGIC_HEADER.as_bytes());

        // parse return status code
        let (payload_len, status_code, payload) = match item {
            // defined status codes from 0 to 3
            StatusCode::Ok(payload) => (payload.len(), 0, Some(payload)),
            StatusCode::UnknownError => (0, 1, None),
            StatusCode::MessageTooLarge => (0, 2, None),
            StatusCode::UnsupportedRequestType => (0, 3, None),
            // reserved status codes from 4 to 32
            // implementation specific status codes start at 33
            StatusCode::EmptyBuffer => (0, 33, None),
            StatusCode::NonEmptyBuffer => (0, 34, None),
            StatusCode::NonAscii => (0, 35, None),
            StatusCode::NonAlphabetic => (0, 36, None),
            StatusCode::NonLowerCase => (0, 37, None),
            // we'll pass back IO errors as an unknown error status code
            StatusCode::IoError(_) => (0, 1, None),
        };

        // write payload length
        dst.put_u16(payload_len as u16); // uses big-endian order

        // write status_code
        dst.put_u16(status_code); // uses big-endian order

        // write payload if needed
        if let Some(payload) = payload {
            dst.reserve(payload.len()); // make space for payload
            dst.put(payload);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    fn max_payload_len_too_small() {
        let _ = PacketCodec::new_with_max_payload(4 * 1024 - 1);
    }

    #[test]
    #[should_panic]
    fn max_payload_len_too_large() {
        let _ = PacketCodec::new_with_max_payload(32 * 1024);
    }

    #[test]
    fn max_payload_len_just_right() {
        let _ = PacketCodec::new_with_max_payload(16 * 1024);
    }

    #[test]
    fn bad_request() {
        let mut codec = PacketCodec::new_with_max_payload(16 * 1024);
        assert_eq!(
            codec.decode(&mut BytesMut::from(&b"STRY\x00\x00\x00\x00"[..])),
            Err(StatusCode::UnsupportedRequestType)
        );
    }

    #[test]
    fn good_ping() {
        let mut codec = PacketCodec::new_with_max_payload(16 * 1024);
        assert_eq!(
            codec.decode(&mut BytesMut::from(&b"STRY\x00\x00\x00\x01"[..])),
            Ok(Some(RequestCode::Ping))
        );
    }

    #[test]
    fn bad_ping() {
        let mut codec = PacketCodec::new_with_max_payload(16 * 1024);
        assert_eq!(
            codec.decode(&mut BytesMut::from(&b"STRY\x00\x01\x00\x01"[..])),
            Err(StatusCode::NonEmptyBuffer)
        );
    }

    #[test]
    fn good_get_stats() {
        let mut codec = PacketCodec::new_with_max_payload(16 * 1024);
        assert_eq!(
            codec.decode(&mut BytesMut::from(&b"STRY\x00\x00\x00\x02"[..])),
            Ok(Some(RequestCode::GetStats))
        );
    }

    #[test]
    fn bad_get_stats() {
        let mut codec = PacketCodec::new_with_max_payload(16 * 1024);
        assert_eq!(
            codec.decode(&mut BytesMut::from(&b"STRY\x00\x01\x00\x02"[..])),
            Err(StatusCode::NonEmptyBuffer)
        );
    }

    #[test]
    fn good_reset_stats() {
        let mut codec = PacketCodec::new_with_max_payload(16 * 1024);
        assert_eq!(
            codec.decode(&mut BytesMut::from(&b"STRY\x00\x00\x00\x03"[..])),
            Ok(Some(RequestCode::ResetStats))
        );
    }

    #[test]
    fn bad_reset_stats() {
        let mut codec = PacketCodec::new_with_max_payload(16 * 1024);
        assert_eq!(
            codec.decode(&mut BytesMut::from(&b"STRY\x00\x01\x00\x03"[..])),
            Err(StatusCode::NonEmptyBuffer)
        );
    }

    #[test]
    fn good_compress() {
        let mut codec = PacketCodec::new_with_max_payload(16 * 1024);
        assert_eq!(
            codec.decode(&mut BytesMut::from(&b"STRY\x00\x05\x00\x04hello"[..])),
            Ok(Some(RequestCode::Compress(BytesMut::from(&b"hello"[..]))))
        );
    }

    #[test]
    fn bad_compress() {
        let mut codec = PacketCodec::new_with_max_payload(16 * 1024);
        assert_eq!(
            codec.decode(&mut BytesMut::from(&b"STRY\x00\x00\x00\x04"[..])),
            Err(StatusCode::EmptyBuffer)
        );
    }

    #[test]
    fn ok() {
        let mut codec = PacketCodec::new_with_max_payload(16 * 1024);
        let mut buffer = BytesMut::new();
        codec
            .encode(StatusCode::Ok(BytesMut::from(&b"hello"[..])), &mut buffer)
            .unwrap();
        assert_eq!(buffer, &b"STRY\x00\x05\x00\x00hello"[..]);
    }

    #[test]
    fn unknown_error() {
        let mut codec = PacketCodec::new_with_max_payload(16 * 1024);
        let mut buffer = BytesMut::new();
        codec
            .encode(StatusCode::UnknownError, &mut buffer)
            .unwrap();
        assert_eq!(buffer, &b"STRY\x00\x00\x00\x01"[..]);
    }

    #[test]
    fn message_to_large() {
        let mut codec = PacketCodec::new_with_max_payload(16 * 1024);
        let mut buffer = BytesMut::new();
        codec
            .encode(StatusCode::MessageTooLarge, &mut buffer)
            .unwrap();
        assert_eq!(buffer, &b"STRY\x00\x00\x00\x02"[..]);
    }

    #[test]
    fn unsupported_request() {
        let mut codec = PacketCodec::new_with_max_payload(16 * 1024);
        let mut buffer = BytesMut::new();
        codec
            .encode(StatusCode::UnsupportedRequestType, &mut buffer)
            .unwrap();
        assert_eq!(buffer, &b"STRY\x00\x00\x00\x03"[..]);
    }

    #[test]
    fn empty_buffer() {
        let mut codec = PacketCodec::new_with_max_payload(16 * 1024);
        let mut buffer = BytesMut::new();
        codec
            .encode(StatusCode::EmptyBuffer, &mut buffer)
            .unwrap();
        assert_eq!(buffer, &b"STRY\x00\x00\x00\x21"[..]);
    }

    #[test]
    fn non_empty_buffer() {
        let mut codec = PacketCodec::new_with_max_payload(16 * 1024);
        let mut buffer = BytesMut::new();
        codec
            .encode(StatusCode::NonEmptyBuffer, &mut buffer)
            .unwrap();
        assert_eq!(buffer, &b"STRY\x00\x00\x00\x22"[..]);
    }

    #[test]
    fn non_ascii() {
        let mut codec = PacketCodec::new_with_max_payload(16 * 1024);
        let mut buffer = BytesMut::new();
        codec
            .encode(StatusCode::NonAscii, &mut buffer)
            .unwrap();
        assert_eq!(buffer, &b"STRY\x00\x00\x00\x23"[..]);
    }

    #[test]
    fn non_alphabetic() {
        let mut codec = PacketCodec::new_with_max_payload(16 * 1024);
        let mut buffer = BytesMut::new();
        codec
            .encode(StatusCode::NonAlphabetic, &mut buffer)
            .unwrap();
        assert_eq!(buffer, &b"STRY\x00\x00\x00\x24"[..]);
    }

    #[test]
    fn non_lowercase() {
        let mut codec = PacketCodec::new_with_max_payload(16 * 1024);
        let mut buffer = BytesMut::new();
        codec
            .encode(StatusCode::NonLowerCase, &mut buffer)
            .unwrap();
        assert_eq!(buffer, &b"STRY\x00\x00\x00\x25"[..]);
    }

    #[test]
    fn io_error() {
        let mut codec = PacketCodec::new_with_max_payload(16 * 1024);
        let mut buffer = BytesMut::new();
        codec
            .encode(StatusCode::IoError(std::io::ErrorKind::Other), &mut buffer)
            .unwrap();
        assert_eq!(buffer, &b"STRY\x00\x00\x00\x01"[..]); // unknown error
    }
}
