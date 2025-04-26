use bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

use super::VideohubMessage;

/// A `tokio_util` Codec for parsing and serializing Videohub protocol messages.
#[derive(Debug, Clone, Default)]
pub struct VideohubCodec;

impl Decoder for VideohubCodec {
    type Item = VideohubMessage;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let input = &src[..];

        match VideohubMessage::parse_single_block(input) {
            Ok((remaining, msg)) => {
                let parsed_len = input.len() - remaining.len();
                src.advance(parsed_len); // Remove the consumed bytes from the buffer
                Ok(Some(msg))
            }
            // Not enough data, wait for more
            Err(nom::Err::Incomplete(_)) => Ok(None),
            // Other error,
            Err(_) => {
                // Parsing error, treat as protocol error
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid Videohub message",
                ))
            }
        }
    }
}

impl Encoder<VideohubMessage> for VideohubCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: VideohubMessage, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let writer = dst.writer();
        item.write_serialized(writer)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{DeviceInfo, Present};
    use super::*;
    use bytes::BytesMut;

    #[test]
    fn decode_simple_message() {
        let mut codec = VideohubCodec::default();
        let input = b"VIDEOHUB DEVICE:\r\nDevice present: true\r\n\r\n";
        let mut buf = BytesMut::from(&input[..]);

        let msg = codec
            .decode(&mut buf)
            .expect("should decode")
            .expect("should have message");

        match msg {
            VideohubMessage::DeviceInfo(DeviceInfo {
                present: Some(Present::Yes),
                ..
            }) => {}
            other => panic!("unexpected message parsed: {:?}", other),
        }

        assert!(buf.is_empty(), "buffer should be fully consumed");
    }
    #[test]
    fn partial_decode() {
        let mut codec = VideohubCodec::default();
        let input = b"VIDEOHUB DEVICE:\r\nDevice present: ";
        let mut buf = BytesMut::from(&input[..]);

        let res = codec.decode(&mut buf).expect("should not error");
        assert!(res.is_none(), "partial input should return None");

        // The buffer should not be consumed yet
        assert_eq!(buf, &input[..]);
    }

    #[test]
    fn encode_simple_message() {
        let mut codec = VideohubCodec::default();
        let msg = VideohubMessage::DeviceInfo(DeviceInfo {
            present: Some(Present::No),
            ..Default::default()
        });

        let mut buf = BytesMut::new();
        codec.encode(msg, &mut buf).expect("should encode");

        let output = String::from_utf8(buf.to_vec()).expect("valid utf8");
        assert!(output.contains("Device present: false"));
        assert!(output.ends_with("\r\n\r\n") || output.ends_with("\n\n"));
    }
}
