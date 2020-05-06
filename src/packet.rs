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
    fn unsupported_request() {
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
}
