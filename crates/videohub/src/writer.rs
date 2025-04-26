// Basic Videohub writer.
// Serializes into the same output as the parser eats.

use super::model::*;
use bytes::{BufMut, BytesMut};
use std::io::{Result, Write};

impl VideohubMessage {
    /// Write a serialized VideohubMessage into a std::io::Writer.
    /// It is terminated by an empty line, completing the block.
    pub fn write_serialized(&self, mut w: impl Write) -> Result<()> {
        match self {
            VideohubMessage::Preamble(p) => {
                write!(w, "PROTOCOL PREAMBLE:\n")?;
                write!(w, "Version: {}\n", p.version)?;
            }
            VideohubMessage::DeviceInfo(d) => {
                write!(w, "VIDEOHUB DEVICE:\n")?;
                macro_rules! opt_val {
                    ($field:expr, $label:expr) => {
                        if let Some(v) = $field {
                            write!(w, "{}: {}\n", $label, v)?;
                        }
                    };
                }

                opt_val!(&d.present, "Device present");
                opt_val!(&d.model_name, "Model name");
                opt_val!(&d.friendly_name, "Friendly name");
                opt_val!(&d.unique_id, "Unique ID");
                opt_val!(d.video_inputs, "Video inputs");
                opt_val!(d.video_processing_units, "Video processing units");
                opt_val!(d.video_outputs, "Video outputs");
                opt_val!(d.video_monitoring_outputs, "Video monitoring outputs");
                opt_val!(d.serial_ports, "Serial ports");

                if let Some(unknown) = &d.unknown_fields {
                    for kv in unknown.iter() {
                        write!(w, "{}: {}\n", &kv.key, &kv.value)?;
                    }
                }
            }
            VideohubMessage::InputLabels(v) => {
                write!(w, "INPUT LABELS:\n")?;
                for l in v {
                    write!(w, "{} {}\n", l.id, l.name)?;
                }
            }
            VideohubMessage::OutputLabels(v) => {
                write!(w, "OUTPUT LABELS:\n")?;
                for l in v {
                    write!(w, "{} {}\n", l.id, l.name)?;
                }
            }
            VideohubMessage::MonitorOutputLabels(v) => {
                write!(w, "MONITOR OUTPUT LABELS:\n")?;
                for l in v {
                    write!(w, "{} {}\n", l.id, l.name)?;
                }
            }
            VideohubMessage::SerialPortLabels(v) => {
                write!(w, "SERIAL PORT LABELS:\n")?;
                for l in v {
                    write!(w, "{} {}\n", l.id, l.name)?;
                }
            }
            VideohubMessage::FrameLabels(v) => {
                write!(w, "FRAME LABELS:\n")?;
                for l in v {
                    write!(w, "{} {}\n", l.id, l.name)?;
                }
            }
            VideohubMessage::VideoOutputRouting(v) => {
                write!(w, "VIDEO OUTPUT ROUTING:\n")?;
                for r in v {
                    write!(w, "{} {}\n", r.to_output, r.from_input)?;
                }
            }
            VideohubMessage::VideoMonitoringOutputRouting(v) => {
                write!(w, "VIDEO MONITORING OUTPUT ROUTING:\n")?;
                for r in v {
                    write!(w, "{} {}\n", r.to_output, r.from_input)?;
                }
            }
            VideohubMessage::SerialPortRouting(v) => {
                write!(w, "SERIAL PORT ROUTING:\n")?;
                for r in v {
                    write!(w, "{} {}\n", r.to_output, r.from_input)?;
                }
            }
            VideohubMessage::ProcessingUnitRouting(v) => {
                write!(w, "PROCESSING UNIT ROUTING:\n")?;
                for r in v {
                    write!(w, "{} {}\n", r.to_output, r.from_input)?;
                }
            }
            VideohubMessage::FrameBufferRouting(v) => {
                write!(w, "FRAME BUFFER ROUTING:\n")?;
                for r in v {
                    write!(w, "{} {}\n", r.to_output, r.from_input)?;
                }
            }
            VideohubMessage::VideoOutputLocks(v) => {
                write!(w, "VIDEO OUTPUT LOCKS:\n")?;
                for l in v {
                    write!(w, "{} {}\n", l.id, l.state)?;
                }
            }
            VideohubMessage::MonitoringOutputLocks(v) => {
                write!(w, "MONITORING OUTPUT LOCKS:\n")?;
                for l in v {
                    write!(w, "{} {}\n", l.id, l.state)?;
                }
            }
            VideohubMessage::SerialPortLocks(v) => {
                write!(w, "SERIAL PORT LOCKS:\n")?;
                for l in v {
                    write!(w, "{} {}\n", l.id, l.state)?;
                }
            }
            VideohubMessage::ProcessingUnitLocks(v) => {
                write!(w, "PROCESSING UNIT LOCKS:\n")?;
                for l in v {
                    write!(w, "{} {}\n", l.id, l.state)?;
                }
            }
            VideohubMessage::FrameBufferLocks(v) => {
                write!(w, "FRAME BUFFER LOCKS:\n")?;
                for l in v {
                    write!(w, "{} {}\n", l.id, l.state)?;
                }
            }
            VideohubMessage::VideoInputStatus(v) => {
                write!(w, "VIDEO INPUT STATUS:\n")?;
                for p in v {
                    write!(w, "{} {:?}\n", p.id, p.port_type)?;
                }
            }
            VideohubMessage::VideoOutputStatus(v) => {
                write!(w, "VIDEO OUTPUT STATUS:\n")?;
                for p in v {
                    write!(w, "{} {:?}\n", p.id, p.port_type)?;
                }
            }
            VideohubMessage::SerialPortStatus(v) => {
                write!(w, "SERIAL PORT STATUS:\n")?;
                for p in v {
                    write!(w, "{} {:?}", p.id, p.port_type)?;
                }
            }
            VideohubMessage::AlarmStatus(v) => {
                write!(w, "ALARM STATUS:\n")?;
                for a in v {
                    write!(w, "{}: {}\n", a.name, a.status)?;
                }
            }
            VideohubMessage::Configuration(v) => {
                write!(w, "CONFIGURATION:\n")?;
                for s in v {
                    write!(w, "{}: {}\n", s.setting, s.value)?;
                }
            }
            VideohubMessage::ACK => {
                write!(w, "ACK\n")?;
            }
            VideohubMessage::NAK => {
                write!(w, "NAK\n")?;
            }
            VideohubMessage::Ping => {
                write!(w, "PING:\n")?;
            }
            VideohubMessage::EndPrelude => {
                write!(w, "END PRELUDE:\n")?;
            }
            VideohubMessage::UnknownMessage(h, body) => {
                w.write_all(&h[..])?;
                w.write_all("\n".as_bytes())?;
                w.write_all(&body[..])?;
            }
        }
        // trailing blank‐line
        write!(w, "\n")?;
        Ok(())
    }

    pub fn to_serialized(&self) -> Result<BytesMut> {
        let mut w = BytesMut::new().writer();
        self.write_serialized(&mut w)?;
        Ok(w.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BMD_EXAMPLE: &[u8] = include_bytes!("./bmd_example.txt");

    #[test]
    fn single_preamble() {
        let m = VideohubMessage::Preamble(Preamble {
            version: "2.4".into(),
        });
        let b = m.to_serialized().unwrap();
        let (r, m2) = VideohubMessage::parse_single_block(&b).unwrap();
        assert!(r.is_empty());
        assert_eq!(m, m2);
    }

    #[test]
    fn single_input_labels() {
        let m = VideohubMessage::InputLabels(vec![
            Label {
                id: 0,
                name: "A".into(),
            },
            Label {
                id: 1,
                name: "B".into(),
            },
        ]);
        let b = m.to_serialized().unwrap();
        let (_, m2) = VideohubMessage::parse_single_block(&b).unwrap();
        assert_eq!(m, m2);
    }

    #[test]
    fn roundtrip_blocks() {
        // parse the real example
        let (_rem, msgs) = VideohubMessage::parse_all_blocks(BMD_EXAMPLE).unwrap();
        // reserialize all
        let mut out = BytesMut::new();
        for m in &msgs {
            out.extend_from_slice(&m.to_serialized().unwrap());
        }
        // parse again
        let (rem2, msgs2) = VideohubMessage::parse_all_blocks(&out).unwrap();
        assert!(rem2.is_empty(), "leftover after round-trip");
        assert_eq!(msgs, msgs2);
    }
}
