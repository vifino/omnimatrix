// BMD Videohub Protocol Data Model

use bytes::BytesMut;
use std::fmt;

/// Preamble contains version.
/// This is only compatible with major version 2, but later minor versions should be compatible.
///
/// ```text
/// PROTOCOL PREAMBLE:↵
/// Version: 2.4↵
/// ↵
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Preamble {
    pub version: String,
}

/// One of:
/// - `Device present: true`
/// - `Device present: false`
/// - `Device present: needs_update`
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum Present {
    Yes,
    #[default]
    No,
    NeedsUpdate,
}

impl fmt::Display for Present {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let v = match self {
            Present::Yes => "true",
            Present::No => "false",
            Present::NeedsUpdate => "needs_update",
        };
        f.write_str(v)
    }
}

/// An unknown Key-Value pair.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UnknownKVPair {
    pub key: String,
    pub value: String,
}

/// VIDEOHUB DEVICE:↵
/// Device present: true↵
/// Model name: Blackmagic Smart Videohub↵
/// Video inputs: 16↵
/// Video processing units: 0↵
/// Video outputs: 16↵
/// Video monitoring outputs: 0↵
/// Serial ports: 0↵
/// ↵
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceInfo {
    pub present: Option<Present>,
    pub model_name: Option<String>,
    pub friendly_name: Option<String>,
    pub unique_id: Option<String>,
    pub video_inputs: Option<u32>,
    pub video_processing_units: Option<u32>,
    pub video_outputs: Option<u32>,
    pub video_monitoring_outputs: Option<u32>,
    pub serial_ports: Option<u32>,
    pub unknown_fields: Option<Vec<UnknownKVPair>>,
}

/// Singular Label of one of the following:
/// - `INPUT LABELS:`
/// - `OUTPUT LABELS:`
/// - `MONITORING OUTPUT LABELS:`
/// - `SERIAL PORT LABELS:`
/// - `FRAME LABELS:`
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Label {
    pub id: u32,
    pub name: String,
}

/// Singular Route of one of the following:
/// - `VIDEO OUTPUT ROUTING:`
/// - `VIDEO MONITORING OUTPUT ROUTING:`
/// - `SERIAL PORT ROUTING:`
/// - `PROCESSING UNIT ROUTING:`
/// - `FRAME BUFFER ROUTING:`
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct Route {
    pub from: u32,
    pub to: u32,
}

/// Lock State
///
/// Represented by something like the following:
/// - `x O` - x is owned by current client
/// - `x L` - x is locked by different client
/// - `x U` - x is not locked
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum LockState {
    /// Lock owned by the current Client
    Owned,
    /// Locked by a different Client
    Locked,
    /// Not locked
    #[default]
    Unlocked,
}

impl fmt::Display for LockState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            LockState::Owned => "O",
            LockState::Locked => "L",
            LockState::Unlocked => "U",
        };
        f.write_str(s)
    }
}

/// A lock of one of the following:
/// - `VIDEO OUTPUT LOCKS:`
/// - `MONITORING OUTPUT LOCKS:`
/// - `SERIAL PORT LOCKS:↵`
/// - `PROCESSING UNIT LOCKS:`
/// - `FRAME BUFFER LOCKS:`
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct Lock {
    pub id: u32,
    pub state: LockState,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum SerialPortDirectionState {
    /// In (Workstation)
    Control,
    /// Out (Deck)
    Slave,
    /// Automatic
    #[default]
    Auto,
}

impl fmt::Display for SerialPortDirectionState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let d = match self {
            SerialPortDirectionState::Control => "control",
            SerialPortDirectionState::Slave => "slave",
            SerialPortDirectionState::Auto => "auto",
        };
        f.write_str(d)
    }
}
/// State of a serial port.
///
/// Transferred like the following:
/// ```text
/// SERIAL PORT DIRECTIONS:↵
/// 0 control↵
/// 1 slave↵
/// 2 auto↵
/// ```
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct SerialPortDirection {
    pub id: u32,
    pub state: SerialPortDirectionState,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum HardwarePortType {
    #[default]
    None,
    BNC,
    Optical,
    /// Undocumented, but it exists.
    Thunderbolt,
    RS422,
    Other(String),
}

impl fmt::Display for HardwarePortType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            HardwarePortType::Other(s) => f.write_str(s),
            _ => fmt::Debug::fmt(self, f),
        }
    }
}

/// A message describing the hardware of the following:
/// - `VIDEO INPUT STATUS:`
/// - `VIDEO OUTPUT STATUS:`
/// - `SERIAL PORT STATUS:`
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HardwarePort {
    pub id: u32,
    pub port_type: HardwarePortType,
}

/// An Alarm Status Message.
/// More akin to sensors, really.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Alarm {
    pub name: String,
    pub status: String,
}

/// An Configuration Message's Setting.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Setting {
    pub setting: String,
    pub value: String,
}

/// Unknown Message.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UnknownMessage {
    pub header: BytesMut,
    pub body: BytesMut,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VideohubMessage {
    /// `PROTOCOL PREAMBLE:`
    Preamble(Preamble),
    /// `VIDEOHUB DEVICE:`
    DeviceInfo(DeviceInfo),

    /// `INPUT LABELS:`
    InputLabels(Vec<Label>),
    /// `OUTPUT LABELS:`
    OutputLabels(Vec<Label>),
    /// `MONITOR OUTPUT LABELS:`
    MonitorOutputLabels(Vec<Label>),
    /// `SERIAL PORT LABELS:`
    SerialPortLabels(Vec<Label>),
    /// `FRAME LABELS:`
    FrameLabels(Vec<Label>),

    /// `VIDEO OUTPUT ROUTING:`
    VideoOutputRouting(Vec<Route>),
    /// `VIDEO MONITORING OUTPUT ROUTING:`
    VideoMonitoringOutputRouting(Vec<Route>),
    /// `SERIAL PORT ROUTING:`
    SerialPortRouting(Vec<Route>),
    /// `PROCESSING UNIT ROUTING:`
    ProcessingUnitRouting(Vec<Route>),
    /// `FRAME BUFFER ROUTING:`
    FrameBufferRouting(Vec<Route>),

    /// `VIDEO OUTPUT LOCKS:`
    VideoOutputLocks(Vec<Lock>),
    /// `MONITORING OUTPUT LOCKS:`
    MonitoringOutputLocks(Vec<Lock>),
    /// `SERIAL PORT LOCKS:`
    SerialPortLocks(Vec<Lock>),
    /// `PROCESSING UNIT LOCKS:`
    ProcessingUnitLocks(Vec<Lock>),
    /// `FRAME BUFFER LOCKS:`
    FrameBufferLocks(Vec<Lock>),

    /// `VIDEO INPUT STATUS:`
    VideoInputStatus(Vec<HardwarePort>),
    /// `VIDEO OUTPUT STATUS:`
    VideoOutputStatus(Vec<HardwarePort>),
    /// `SERIAL PORT STATUS:`
    SerialPortStatus(Vec<HardwarePort>),

    /// `ALARM STATUS:`
    AlarmStatus(Vec<Alarm>),
    /// `CONFIGURATION:` (at least ver 2.7)
    Configuration(Vec<Setting>),

    /// `ACK`
    ACK,
    /// `NAK`
    NAK,
    /// `PING:`
    Ping,
    /// `END PRELUDE:`
    EndPrelude,

    /// Unknown Message
    UnknownMessage(BytesMut, BytesMut),
}
