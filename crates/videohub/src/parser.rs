// Basic Video Hub Parser.

use crate::helpers::*;
use crate::model::*;
use bytes::BytesMut;
use nom::{
    bytes::streaming::{tag, take_until},
    character::streaming::{multispace0, space1},
    error::{Error, ErrorKind, ParseError},
    sequence::{preceded, terminated, tuple},
    Err, IResult,
};

const COLON: &[u8] = b":";

/// Parse one "Key: Value" line to (key, value) tuple
fn parse_kv_line(i: &[u8]) -> IResult<&[u8], (&[u8], &[u8])> {
    let (i, (k, _, _, v, _)) = tuple((
        take_until(COLON),
        tag(COLON),
        space1,
        take_until_newline,
        any_newline,
    ))(i)?;
    Ok((i, (k.trim_ascii(), v.trim_ascii_end())))
}

/// Parse the body of a Preamble block after its header
fn parse_preamble_body(i: &[u8]) -> IResult<&[u8], VideohubMessage> {
    let (i, (_, _, ver, _)) =
        tuple((tag(b"Version"), tag(COLON), take_until_newline, any_newline))(i)?;
    let version = String::from_utf8_lossy(ver.trim_ascii()).to_string();
    Ok((i, VideohubMessage::Preamble(Preamble { version })))
}

/// Parse the body of DeviceInfo block after its header
fn parse_device_body(mut i: &[u8]) -> IResult<&[u8], VideohubMessage> {
    let mut di = DeviceInfo::default();
    while let Ok((i2, (k, v))) = parse_kv_line(i) {
        match k {
            b"Device present" => {
                di.present = Some(match v {
                    b"true" => Present::Yes,
                    b"false" => Present::No,
                    b"needs_update" => Present::NeedsUpdate,
                    _ => return Err(Err::Error(Error::from_error_kind(i, ErrorKind::Tag))),
                })
            }
            b"Model name" => di.model_name = Some(String::from_utf8_lossy(v).to_string()),
            b"Friendly name" => di.unique_id = Some(String::from_utf8_lossy(v).to_string()),
            b"Unique ID" => di.unique_id = Some(String::from_utf8_lossy(v).to_string()),
            b"Video inputs" => di.video_inputs = Some(parse_u32(v)?.1),
            b"Video processing units" => di.video_processing_units = Some(parse_u32(v)?.1),
            b"Video outputs" => di.video_outputs = Some(parse_u32(v)?.1),
            b"Video monitoring outputs" => di.video_monitoring_outputs = Some(parse_u32(v)?.1),
            b"Serial ports" => di.serial_ports = Some(parse_u32(v)?.1),
            key => {
                let mut unknown = di.unknown_fields.unwrap_or_else(|| Vec::new());
                unknown.push(UnknownKVPair {
                    key: String::from_utf8_lossy(key).to_string(),
                    value: String::from_utf8_lossy(v).to_string(),
                });
                di.unknown_fields = Some(unknown);
            }
        }
        i = i2;
    }
    Ok((i, VideohubMessage::DeviceInfo(di)))
}

/// Parse generic "ID Name Here" label lines
fn parse_label_body<'a>(
    mut i: &'a [u8],
    ctor: fn(Vec<Label>) -> VideohubMessage,
) -> IResult<&'a [u8], VideohubMessage> {
    let mut out = Vec::new();
    while let Ok((i2, (id, _, nm, _))) =
        tuple((parse_u32, space1, take_until_newline, any_newline))(i)
    {
        out.push(Label {
            id,
            name: String::from_utf8_lossy(nm.trim_ascii()).to_string(),
        });
        i = i2;
    }
    Ok((i, ctor(out)))
}

/// Parse generic "to from" route lines
fn parse_route_body<'a>(
    mut i: &'a [u8],
    ctor: fn(Vec<Route>) -> VideohubMessage,
) -> IResult<&'a [u8], VideohubMessage> {
    let mut out = Vec::new();
    while let Ok((i2, (t, _, f, _))) = tuple((parse_u32, space1, parse_u32, any_newline))(i) {
        out.push(Route { from_input: f, to_output: t });
        i = i2;
    }
    Ok((i, ctor(out)))
}

/// Parse generic "ID [O/L/U]" lines
fn parse_lock_body<'a>(
    mut i: &'a [u8],
    ctor: fn(Vec<Lock>) -> VideohubMessage,
) -> IResult<&'a [u8], VideohubMessage> {
    let mut out = Vec::new();
    while let Ok((i2, (id, _, s, _))) =
        tuple((parse_u32, space1, take_until_newline, any_newline))(i)
    {
        let state = match s.trim_ascii_end() {
            b"O" => LockState::Owned,
            b"L" => LockState::Locked,
            b"U" => LockState::Unlocked,
            _ => return Err(Err::Error(Error::from_error_kind(i, ErrorKind::Tag))),
        };
        out.push(Lock { id, state });
        i = i2;
    }
    Ok((i, ctor(out)))
}

/// Parse generic "status" lines
fn parse_hw_body<'a>(
    mut i: &'a [u8],
    ctor: fn(Vec<HardwarePort>) -> VideohubMessage,
) -> IResult<&'a [u8], VideohubMessage> {
    let mut out = Vec::new();
    while let Ok((i2, (id, _, tp, _))) =
        tuple((parse_u32, space1, take_until_newline, any_newline))(i)
    {
        let port_type = match tp.trim_ascii_end() {
            b"None" => HardwarePortType::None,
            b"BNC" => HardwarePortType::BNC,
            b"Optical" => HardwarePortType::Optical,
            b"Thunderbolt" => HardwarePortType::Thunderbolt,
            b"RS422" => HardwarePortType::RS422,
            unknown => HardwarePortType::Other(String::from_utf8_lossy(unknown).to_string()),
        };
        out.push(HardwarePort { id, port_type });
        i = i2;
    }
    Ok((i, ctor(out)))
}

/// Parse generic Key-Value lines
fn parse_kv_body<'a>(
    mut i: &'a [u8],
    ctor: fn(Vec<(&'a [u8], &'a [u8])>) -> VideohubMessage,
) -> IResult<&'a [u8], VideohubMessage> {
    let mut out = Vec::new();
    while let Ok((i2, (k, v))) = parse_kv_line(i) {
        out.push((k, v));
        i = i2;
    }
    Ok((i, ctor(out)))
}

impl VideohubMessage {
    /// Parse one block including its trailing blank-line
    pub fn parse_single_block(i: &[u8]) -> IResult<&[u8], VideohubMessage> {
        let (i, header) = preceded(multispace0, terminated(take_until_newline, any_newline))(i)?;
        let (i, body) = take_until_empty_line(i)?;
        let (_, msg) = match header.trim_ascii_end() {
            b"PROTOCOL PREAMBLE:" => parse_preamble_body(body)?,
            b"VIDEOHUB DEVICE:" => parse_device_body(body)?,

            b"INPUT LABELS:" => parse_label_body(body, VideohubMessage::InputLabels)?,
            b"OUTPUT LABELS:" => parse_label_body(body, VideohubMessage::OutputLabels)?,
            b"MONITOR OUTPUT LABELS:" => {
                parse_label_body(body, VideohubMessage::MonitorOutputLabels)?
            }
            b"SERIAL PORT LABELS:" => parse_label_body(body, VideohubMessage::SerialPortLabels)?,
            b"FRAME LABELS:" => parse_label_body(body, VideohubMessage::FrameLabels)?,

            b"VIDEO OUTPUT ROUTING:" => {
                parse_route_body(body, VideohubMessage::VideoOutputRouting)?
            }
            b"VIDEO MONITORING OUTPUT ROUTING:" => {
                parse_route_body(body, VideohubMessage::VideoMonitoringOutputRouting)?
            }
            b"SERIAL PORT ROUTING:" => parse_route_body(body, VideohubMessage::SerialPortRouting)?,
            b"PROCESSING UNIT ROUTING:" => {
                parse_route_body(body, VideohubMessage::ProcessingUnitRouting)?
            }
            b"FRAME BUFFER ROUTING:" => {
                parse_route_body(body, VideohubMessage::FrameBufferRouting)?
            }

            b"VIDEO OUTPUT LOCKS:" => parse_lock_body(body, VideohubMessage::VideoOutputLocks)?,
            b"MONITORING OUTPUT LOCKS:" => {
                parse_lock_body(body, VideohubMessage::MonitoringOutputLocks)?
            }
            b"SERIAL PORT LOCKS:" => parse_lock_body(body, VideohubMessage::SerialPortLocks)?,
            b"PROCESSING UNIT LOCKS:" => {
                parse_lock_body(body, VideohubMessage::ProcessingUnitLocks)?
            }
            b"FRAME BUFFER LOCKS:" => parse_lock_body(body, VideohubMessage::FrameBufferLocks)?,

            b"VIDEO INPUT STATUS:" => parse_hw_body(body, VideohubMessage::VideoInputStatus)?,
            b"VIDEO OUTPUT STATUS:" => parse_hw_body(body, VideohubMessage::VideoOutputStatus)?,
            b"SERIAL PORT STATUS:" => parse_hw_body(body, VideohubMessage::SerialPortStatus)?,

            b"ALARM STATUS:" => parse_kv_body(body, |vals| {
                VideohubMessage::AlarmStatus(
                    vals.iter()
                        .map(|t| Alarm {
                            name: String::from_utf8_lossy(t.0.trim_ascii()).to_string(),
                            status: String::from_utf8_lossy(t.1.trim_ascii()).to_string(),
                        })
                        .collect(),
                )
            })?,
            b"CONFIGURATION:" => parse_kv_body(body, |vals| {
                VideohubMessage::Configuration(
                    vals.iter()
                        .map(|t| Setting {
                            setting: String::from_utf8_lossy(t.0.trim_ascii()).to_string(),
                            value: String::from_utf8_lossy(t.1.trim_ascii()).to_string(),
                        })
                        .collect(),
                )
            })?,

            b"ACK" => (i, VideohubMessage::ACK),
            b"NAK" => (i, VideohubMessage::ACK),
            b"PING:" => (i, VideohubMessage::Ping),
            b"END PRELUDE:" => (i, VideohubMessage::EndPrelude),

            header => (
                b"".as_slice(),
                VideohubMessage::UnknownMessage(BytesMut::from(header), BytesMut::from(body)),
            ),
        };
        Ok((i, msg))
    }

    /// Parse an entire Videohub conversation of multiple messages.
    pub fn parse_all_blocks(input: &[u8]) -> IResult<&[u8], Vec<VideohubMessage>> {
        let mut i = input;
        let mut messages = Vec::new();
        loop {
            let (ni, message) = Self::parse_single_block(i)?;
            messages.push(message);
            if ni.is_empty() {
                return Ok((ni, messages));
            }
            i = ni;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static BMD_EXAMPLE: &[u8] = include_bytes!("./bmd_example.txt");

    #[test]
    fn parse_only_preamble() {
        let buf = b"PROTOCOL PREAMBLE:\r\nVersion: 2.4\r\n\r\n";
        let (rem, msg) = VideohubMessage::parse_single_block(buf).expect("should parse preamble");
        // no leftover
        assert!(rem.is_empty(), "remaining = {:?}", rem);
        match msg {
            VideohubMessage::Preamble(p) => assert_eq!(p.version, "2.4"),
            _ => panic!("expected Preamble, got {:?}", msg),
        }
    }

    #[test]
    fn parse_only_deviceinfo() {
        let buf = b"VIDEOHUB DEVICE:\r\n\
                    Device present: true\r\n\
                    Model name: FooBar\r\n\
                    Video inputs: 3\r\n\r\n";
        let (rem, msg) = VideohubMessage::parse_single_block(buf).expect("should parse device");
        assert!(rem.is_empty(), "remaining = {:?}", rem);
        match msg {
            VideohubMessage::DeviceInfo(d) => {
                assert!(matches!(d.present, Some(Present::Yes)));
                assert_eq!(d.model_name.as_deref(), Some("FooBar"));
                assert_eq!(d.video_inputs, Some(3));
            }
            _ => panic!("expected DeviceInfo, got {:?}", msg),
        }
    }

    #[test]
    fn parse_only_input_labels() {
        let buf = b"INPUT LABELS:\r\n0 A\r\n1 B\r\n\r\n";
        let (rem, msg) =
            VideohubMessage::parse_single_block(buf).expect("should parse input labels");
        assert!(rem.is_empty(), "remaining = {:?}", rem);
        match msg {
            VideohubMessage::InputLabels(v) => {
                assert_eq!(v.len(), 2);
                assert_eq!(v[0].id, 0);
                assert_eq!(&v[0].name, "A");
                assert_eq!(v[1].id, 1);
                assert_eq!(&v[1].name, "B");
            }
            _ => panic!("expected InputLabels, got {:?}", msg),
        }
    }

    #[test]
    fn parse_only_output_labels() {
        let buf = b"OUTPUT LABELS:\n5 X\n\n";
        let (rem, msg) =
            VideohubMessage::parse_single_block(buf).expect("should parse output labels");
        assert!(rem.is_empty(), "remaining = {:?}", rem);
        match msg {
            VideohubMessage::OutputLabels(v) => {
                assert_eq!(v.len(), 1);
                assert_eq!(v[0].id, 5);
                assert_eq!(&v[0].name, "X");
            }
            _ => panic!("expected OutputLabels, got {:?}", msg),
        }
    }

    #[test]
    fn parse_partial() {
        let mut buf: Vec<u8> = Vec::from(b"INPUT ");
        let r = VideohubMessage::parse_single_block(&buf);
        assert!(r.is_err());

        buf.extend_from_slice(b"LABELS:\n0 A");
        let r = VideohubMessage::parse_single_block(&buf);
        assert!(r.is_err());

        buf.extend_from_slice(b"\n\nOUTPUT LABELS:\n");
        let (rem, partial) = VideohubMessage::parse_single_block(&buf).unwrap();
        assert_eq!(
            partial,
            VideohubMessage::InputLabels(vec![Label {
                id: 0,
                name: String::from("A"),
            }])
        );
        assert_eq!(rem, b"OUTPUT LABELS:\n");
    }

    #[test]
    fn parse_multiple_sections() {
        let buf = b"PROTOCOL PREAMBLE:\nVersion:2.4\n\nINPUT LABELS:\n0 A\n\n";
        let (rem, v) = VideohubMessage::parse_all_blocks(buf).expect("should parse two sections");
        assert!(rem.is_empty(), "remaining = {:?}", rem);
        assert_eq!(v.len(), 2);
        matches!(v[0], VideohubMessage::Preamble(_));
        matches!(v[1], VideohubMessage::InputLabels(_));
    }

    #[test]
    fn parse_bmd_example() {
        let (rem, msgs) = VideohubMessage::parse_all_blocks(BMD_EXAMPLE).unwrap();
        assert!(rem.is_empty(), "remaining = {:?}", rem);
        assert_eq!(msgs.len(), 4);

        match &msgs[0] {
            VideohubMessage::Preamble(p) => assert_eq!(p.version, "2.4"),
            _ => panic!("expected Preamble"),
        }
        match &msgs[1] {
            VideohubMessage::DeviceInfo(d) => assert!(matches!(d.present, Some(Present::Yes))),
            _ => panic!("expected DeviceInfo"),
        }
        match &msgs[2] {
            VideohubMessage::InputLabels(v) => {
                assert_eq!(v[0].id, 0);
                assert_eq!(&v[0].name, "Camera 1");
            }
            _ => panic!("expected InputLabels"),
        }
        match &msgs[3] {
            VideohubMessage::OutputLabels(v) => {
                assert_eq!(v[0].id, 0);
                assert_eq!(&v[0].name, "Main Monitor 1");
            }
            _ => panic!("expected OutputLabels"),
        }
    }
}
