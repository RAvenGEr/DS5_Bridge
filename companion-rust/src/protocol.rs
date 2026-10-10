use serde::{Deserialize, Serialize};

pub const MAGIC: [u8; 4] = [b'D', b'S', b'5', b'B'];
pub const PROTOCOL_MAJOR: u8 = 1;
pub const PROTOCOL_MINOR: u8 = 23;
pub const REPORT_LENGTH: usize = 64;

pub const REPORT_ID_STATUS: u8 = 0x01;
pub const REPORT_ID_COMMAND: u8 = 0x02;
pub const REPORT_ID_ACK: u8 = 0x03;

/// Command ids, mirroring `COMMAND_ID` in companion/src/shared/protocol.ts and
/// the `Command*` enum in src/companion.cpp. Commands the Rust companion does
/// not use yet are kept so future features do not have to re-derive them.
#[allow(dead_code)]
mod command_ids {
    pub const COMMAND_ID_SET_HAPTICS_GAIN: u8 = 0x01;
    pub const COMMAND_ID_SET_LED_ENABLED: u8 = 0x02;
    pub const COMMAND_ID_SET_IDLE_DISCONNECT_ENABLED: u8 = 0x03;
    pub const COMMAND_ID_TEST_HAPTICS: u8 = 0x04;
    pub const COMMAND_ID_RESTORE_DEFAULTS: u8 = 0x05;
    pub const COMMAND_ID_SET_SPEAKER_VOLUME: u8 = 0x07;
    pub const COMMAND_ID_SET_LIGHTBAR_COLOR: u8 = 0x08;
    pub const COMMAND_ID_SET_LIGHTBAR_OVERRIDE: u8 = 0x09;
    pub const COMMAND_ID_SET_MUTE_BUTTON_ACTION: u8 = 0x0A;
    pub const COMMAND_ID_SET_HAPTICS_BUFFER_LENGTH: u8 = 0x0B;
    pub const COMMAND_ID_SET_TRIGGER_EFFECT_INTENSITY: u8 = 0x0C;
    pub const COMMAND_ID_TEST_ADAPTIVE_TRIGGERS: u8 = 0x0D;
    pub const COMMAND_ID_RESET_ADAPTIVE_TRIGGERS: u8 = 0x0E;
    pub const COMMAND_ID_SET_USB_SUSPEND_DISCONNECT_ENABLED: u8 = 0x0F;
    pub const COMMAND_ID_SET_SLEEP_KEYBIND_ENABLED: u8 = 0x10;
    pub const COMMAND_ID_SLEEP_CONTROLLER: u8 = 0x11;
    pub const COMMAND_ID_SET_POLLING_RATE_MODE: u8 = 0x12;
    pub const COMMAND_ID_SET_CLASSIC_RUMBLE_GAIN: u8 = 0x13;
    pub const COMMAND_ID_TEST_CLASSIC_RUMBLE: u8 = 0x14;
    pub const COMMAND_ID_SET_DUPLEX_ENABLED: u8 = 0x19;
    pub const COMMAND_ID_SET_MIC_VOLUME: u8 = 0x1A;
    pub const COMMAND_ID_SET_MIC_MUTE: u8 = 0x1B;
    pub const COMMAND_ID_SET_IDLE_DISCONNECT_TIMEOUT: u8 = 0x1C;
    pub const COMMAND_ID_SET_SPEAKER_VOLUME_SHORTCUT_ENABLED: u8 = 0x1D;
    pub const COMMAND_ID_SET_BUTTON_REMAP: u8 = 0x1E;
    pub const COMMAND_ID_PREVIEW_ADAPTIVE_TRIGGER_EFFECT: u8 = 0x1F;
    pub const COMMAND_ID_APPLY_ADAPTIVE_TRIGGER_EFFECT: u8 = 0x20;
    pub const COMMAND_ID_SET_HOST_PERSONA: u8 = 0x21;
    pub const COMMAND_ID_SET_AUDIO_REACTIVE_HAPTICS: u8 = 0x22;
    pub const COMMAND_ID_SET_CHORD_BINDINGS: u8 = 0x23;
    pub const COMMAND_ID_SET_PLAYER_LED_ENABLED: u8 = 0x24;
    pub const COMMAND_ID_SET_CLASSIC_RUMBLE_V1: u8 = 0x25;
    pub const COMMAND_ID_REQUEST_CONTROLLER_SCAN: u8 = 0x27;
    pub const COMMAND_ID_FORGET_CONTROLLER_PAIRINGS: u8 = 0x28;
    pub const COMMAND_ID_FORGET_CONTROLLER_PAIRING: u8 = 0x2E;
    pub const COMMAND_ID_SET_SPEAKER_GAIN: u8 = 0x32;
    pub const COMMAND_ID_ENTER_BOOTLOADER: u8 = 0x33;
    pub const COMMAND_ID_SET_AUDIO_INTERLEAVE: u8 = 0x34;
    pub const COMMAND_ID_SET_WAKE_ON_CONNECT: u8 = 0x35;
    pub const COMMAND_ID_SET_LIGHTBAR_RESTORE_ENABLED: u8 = 0x36;
    pub const COMMAND_ID_SET_RADIAL_DEADZONES: u8 = 0x37;
    pub const COMMAND_ID_SET_EDGE_PROFILE_SWITCHING_BLOCKED: u8 = 0x45;
}
pub use command_ids::*;

pub const MIN_IDLE_DISCONNECT_TIMEOUT_MINUTES: u16 = 1;
pub const MAX_IDLE_DISCONNECT_TIMEOUT_MINUTES: u16 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostPersonaMode {
    DualSense,
    DualSenseEdge,
    Ds4,
    Xbox,
}

impl HostPersonaMode {
    pub fn to_protocol_value(self) -> u8 {
        match self {
            HostPersonaMode::DualSense => 0,
            HostPersonaMode::Xbox => 1,
            HostPersonaMode::Ds4 => 2,
            HostPersonaMode::DualSenseEdge => 7,
        }
    }

    pub fn from_protocol_value(val: u8) -> Self {
        match val {
            1 => HostPersonaMode::Xbox,
            2 => HostPersonaMode::Ds4,
            7 => HostPersonaMode::DualSenseEdge,
            _ => HostPersonaMode::DualSense,
        }
    }

    /// Whether the bridge advertises support for this mode in the status
    /// report's `supported_host_persona_mask` byte.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_supported_by(self, mask: u8) -> bool {
        mask & (1 << self.to_protocol_value()) != 0
    }

    pub fn as_str(self) -> &'static str {
        match self {
            HostPersonaMode::DualSense => "dualsense",
            HostPersonaMode::DualSenseEdge => "dualsense-edge",
            HostPersonaMode::Ds4 => "ds4",
            HostPersonaMode::Xbox => "xbox",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            HostPersonaMode::DualSense => "DualSense (PS5)",
            HostPersonaMode::DualSenseEdge => "DualSense Edge",
            HostPersonaMode::Ds4 => "DualShock 4 (PS4)",
            HostPersonaMode::Xbox => "Xbox 360 (XInput)",
        }
    }

    pub fn parse_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "xbox" | "xinput" | "360" => HostPersonaMode::Xbox,
            "ds4" | "ps4" | "dualshock" => HostPersonaMode::Ds4,
            "dualsense-edge" | "edge" => HostPersonaMode::DualSenseEdge,
            _ => HostPersonaMode::DualSense,
        }
    }
}

/// Result codes reported by the bridge in an ACK report
/// (`ACK_RESULT` in companion/src/shared/protocol.ts, `AckResult` in
/// src/companion.cpp).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AckResult {
    Ok,
    ErrBadMagic,
    ErrBadVersion,
    ErrBadLength,
    ErrInvalidValue,
    ErrUnknownCommand,
    ErrNotConnected,
    ErrBusy,
    ErrPersistenceFailed,
    Unknown(u8),
}

impl AckResult {
    pub fn from_u8(value: u8) -> Self {
        match value {
            0x00 => AckResult::Ok,
            0x01 => AckResult::ErrBadMagic,
            0x02 => AckResult::ErrBadVersion,
            0x03 => AckResult::ErrBadLength,
            0x04 => AckResult::ErrInvalidValue,
            0x05 => AckResult::ErrUnknownCommand,
            0x06 => AckResult::ErrNotConnected,
            0x07 => AckResult::ErrBusy,
            0x08 => AckResult::ErrPersistenceFailed,
            other => AckResult::Unknown(other),
        }
    }

    pub fn is_ok(self) -> bool {
        matches!(self, AckResult::Ok)
    }

    /// User-facing message, mirroring `ackUserMessage()` in the Electron
    /// companion.
    pub fn user_message(self) -> String {
        match self {
            AckResult::Ok => "OK".to_string(),
            AckResult::ErrNotConnected => "Controller not connected".to_string(),
            AckResult::ErrBusy => "Bridge is busy".to_string(),
            AckResult::ErrPersistenceFailed => {
                "Firmware could not persist settings".to_string()
            }
            AckResult::ErrInvalidValue => "Invalid value".to_string(),
            AckResult::ErrBadVersion => "Firmware protocol mismatch".to_string(),
            AckResult::ErrBadMagic => "Bad report magic".to_string(),
            AckResult::ErrBadLength => "Bad report length".to_string(),
            AckResult::ErrUnknownCommand => "Unknown command".to_string(),
            AckResult::Unknown(code) => format!("Unknown result 0x{:02x}", code),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolError {
    BadLength(usize),
    BadReportId { expected: u8, actual: u8 },
    BadMagic,
    VersionMismatch { major: u8, minor: u8 },
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtocolError::BadLength(len) => {
                write!(f, "Expected {} bytes, received {}.", REPORT_LENGTH, len)
            }
            ProtocolError::BadReportId { expected, actual } => write!(
                f,
                "Expected report ID 0x{:02x}, received 0x{:02x}.",
                expected, actual
            ),
            ProtocolError::BadMagic => write!(f, "Companion report magic did not match DS5B."),
            ProtocolError::VersionMismatch { major, minor } => write!(
                f,
                "Firmware update required. Expected companion protocol {}.{}, received {}.{}.",
                PROTOCOL_MAJOR, PROTOCOL_MINOR, major, minor
            ),
        }
    }
}

impl std::error::Error for ProtocolError {}

#[derive(Debug, Clone, PartialEq)]
pub struct AckPayload {
    pub command_id: u8,
    pub sequence: u8,
    pub result: AckResult,
    pub detail: u8,
    pub settings_revision: u16,
    pub uptime_seconds: u32,
    pub protocol_major: u8,
    pub protocol_minor: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BridgeStatusPayload {
    pub controller_connected: bool,
    pub controller_type: String,
    pub battery_percent: Option<u8>,
    pub raw_power_state: u8,
    pub host_persona_mode: HostPersonaMode,
    /// Bitmap of host persona modes the bridge firmware can switch to.
    pub supported_persona_mask: u8,
    pub firmware_version: String,
    pub protocol_major: u8,
    pub protocol_minor: u8,
    pub uptime_seconds: u32,
    pub settings_revision: u16,
    pub last_command_result: AckResult,
    // Power / sleep related settings, mirrored from the status report so the
    // UI reflects the device rather than a local guess.
    pub led_enabled: bool,
    pub idle_disconnect_enabled: bool,
    pub idle_disconnect_timeout_minutes: u16,
    pub usb_suspend_disconnect_enabled: bool,
    pub sleep_keybind_enabled: bool,
    pub wake_on_connect_enabled: bool,
}

/// Validates report id, magic and protocol version for any inbound report.
fn validate_report(report: &[u8], expected_id: u8) -> Result<(), ProtocolError> {
    if report.len() < REPORT_LENGTH {
        return Err(ProtocolError::BadLength(report.len()));
    }
    if report[0] != expected_id {
        return Err(ProtocolError::BadReportId {
            expected: expected_id,
            actual: report[0],
        });
    }
    if report[1..5] != MAGIC {
        return Err(ProtocolError::BadMagic);
    }
    if report[5] != PROTOCOL_MAJOR || report[6] != PROTOCOL_MINOR {
        return Err(ProtocolError::VersionMismatch {
            major: report[5],
            minor: report[6],
        });
    }
    Ok(())
}

fn read_u16(report: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([report[offset], report[offset + 1]])
}

fn read_u32(report: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        report[offset],
        report[offset + 1],
        report[offset + 2],
        report[offset + 3],
    ])
}

pub fn build_command_report(
    command_id: u8,
    sequence: u8,
    value: u16,
    extra: &[u8],
) -> [u8; REPORT_LENGTH] {
    let mut report = [0u8; REPORT_LENGTH];
    report[0] = REPORT_ID_COMMAND;
    report[1] = MAGIC[0];
    report[2] = MAGIC[1];
    report[3] = MAGIC[2];
    report[4] = MAGIC[3];
    report[5] = PROTOCOL_MAJOR;
    report[6] = PROTOCOL_MINOR;
    report[7] = command_id & 0xff;
    report[8] = sequence & 0xff;
    report[9] = (value & 0xff) as u8;
    report[10] = ((value >> 8) & 0xff) as u8;

    for (i, &b) in extra.iter().enumerate() {
        if 11 + i < REPORT_LENGTH {
            report[11 + i] = b;
        }
    }
    report
}

/// Builds a command whose only payload is its 16-bit value (0/1 toggles, enum
/// valued settings, ...).
pub fn build_value_report(command_id: u8, sequence: u8, value: u16) -> [u8; REPORT_LENGTH] {
    build_command_report(command_id, sequence, value, &[])
}

/// Builds a boolean on/off setting command.
pub fn build_bool_report(command_id: u8, sequence: u8, enabled: bool) -> [u8; REPORT_LENGTH] {
    build_value_report(command_id, sequence, enabled as u16)
}

pub fn build_set_persona_report(sequence: u8, mode: HostPersonaMode) -> [u8; REPORT_LENGTH] {
    build_value_report(
        COMMAND_ID_SET_HOST_PERSONA,
        sequence,
        mode.to_protocol_value() as u16,
    )
}

/// `SLEEP_CONTROLLER` requires a zero value (see `CommandSleepController` in
/// src/companion.cpp); the bridge replies `AckNotConnected` when no controller
/// is paired/connected.
pub fn build_sleep_controller_report(sequence: u8) -> [u8; REPORT_LENGTH] {
    build_value_report(COMMAND_ID_SLEEP_CONTROLLER, sequence, 0)
}

pub fn clamp_idle_disconnect_timeout(minutes: u16) -> u16 {
    minutes.clamp(
        MIN_IDLE_DISCONNECT_TIMEOUT_MINUTES,
        MAX_IDLE_DISCONNECT_TIMEOUT_MINUTES,
    )
}

pub fn build_idle_disconnect_timeout_report(sequence: u8, minutes: u16) -> [u8; REPORT_LENGTH] {
    build_value_report(
        COMMAND_ID_SET_IDLE_DISCONNECT_TIMEOUT,
        sequence,
        clamp_idle_disconnect_timeout(minutes),
    )
}

pub fn parse_ack_report(report: &[u8]) -> Result<AckPayload, ProtocolError> {
    validate_report(report, REPORT_ID_ACK)?;
    Ok(AckPayload {
        command_id: report[7],
        sequence: report[8],
        result: AckResult::from_u8(report[9]),
        detail: report[10],
        settings_revision: read_u16(report, 11),
        uptime_seconds: read_u32(report, 13),
        protocol_major: report[5],
        protocol_minor: report[6],
    })
}

pub fn parse_status_report(report: &[u8]) -> Result<BridgeStatusPayload, ProtocolError> {
    validate_report(report, REPORT_ID_STATUS)?;

    let battery_raw = report[9];
    let status_flags = report[20];

    Ok(BridgeStatusPayload {
        controller_connected: report[7] == 1,
        controller_type: match report[8] {
            1 => "DualSense".to_string(),
            2 => "DualSense Edge".to_string(),
            _ => "Unknown Controller".to_string(),
        },
        battery_percent: if battery_raw == 255 { None } else { Some(battery_raw) },
        raw_power_state: report[10],
        host_persona_mode: HostPersonaMode::from_protocol_value(report[48]),
        supported_persona_mask: report[49],
        firmware_version: format!("{}.{}.{}", report[25], report[26], report[27]),
        protocol_major: report[5],
        protocol_minor: report[6],
        uptime_seconds: read_u32(report, 21),
        settings_revision: read_u16(report, 17),
        last_command_result: AckResult::from_u8(report[19]),
        led_enabled: report[15] == 1,
        idle_disconnect_enabled: report[16] == 1,
        idle_disconnect_timeout_minutes: read_u16(report, 43),
        usb_suspend_disconnect_enabled: (status_flags & 0x10) != 0,
        sleep_keybind_enabled: (status_flags & 0x40) != 0,
        wake_on_connect_enabled: report[50] == 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn magic_header(report_id: u8) -> [u8; REPORT_LENGTH] {
        let mut report = [0u8; REPORT_LENGTH];
        report[0] = report_id;
        report[1..5].copy_from_slice(&MAGIC);
        report[5] = PROTOCOL_MAJOR;
        report[6] = PROTOCOL_MINOR;
        report
    }

    fn sample_status() -> [u8; REPORT_LENGTH] {
        let mut report = magic_header(REPORT_ID_STATUS);
        report[7] = 1; // controller connected
        report[8] = 2; // DualSense Edge
        report[9] = 76; // battery %
        report[10] = 0; // raw power state
        report[15] = 1; // led enabled
        report[16] = 1; // idle disconnect enabled
        report[17..19].copy_from_slice(&42u16.to_le_bytes()); // settings revision
        report[19] = 0; // last command result (OK)
        report[20] = 0x10 | 0x40; // usb suspend disconnect + sleep keybind
        report[21..25].copy_from_slice(&3725u32.to_le_bytes()); // uptime
        report[25] = 1;
        report[26] = 7;
        report[27] = 1;
        report[43..45].copy_from_slice(&30u16.to_le_bytes()); // idle timeout
        report[48] = 7; // DualSense Edge active
        report[49] = 0x01 | 0x02 | 0x04 | 0x80; // supported personas
        report[50] = 1; // wake on connect
        report
    }

    #[test]
    fn parses_status_report_fields() {
        let status = parse_status_report(&sample_status()).expect("status parses");

        assert!(status.controller_connected);
        assert_eq!(status.controller_type, "DualSense Edge");
        assert_eq!(status.battery_percent, Some(76));
        assert_eq!(status.host_persona_mode, HostPersonaMode::DualSenseEdge);
        assert_eq!(status.firmware_version, "1.7.1");
        assert_eq!(status.uptime_seconds, 3725);
        assert_eq!(status.settings_revision, 42);
        assert!(status.last_command_result.is_ok());
        assert!(status.led_enabled);
        assert!(status.idle_disconnect_enabled);
        assert_eq!(status.idle_disconnect_timeout_minutes, 30);
        assert!(status.usb_suspend_disconnect_enabled);
        assert!(status.sleep_keybind_enabled);
        assert!(status.wake_on_connect_enabled);
        assert_eq!(status.supported_persona_mask.count_ones(), 4);
    }

    #[test]
    fn rejects_bad_magic_and_report_id() {
        let mut report = sample_status();
        report[2] = b'X';
        assert_eq!(
            parse_status_report(&report),
            Err(ProtocolError::BadMagic)
        );

        let mut report = sample_status();
        report[0] = REPORT_ID_ACK;
        assert_eq!(
            parse_status_report(&report),
            Err(ProtocolError::BadReportId {
                expected: REPORT_ID_STATUS,
                actual: REPORT_ID_ACK
            })
        );

        assert_eq!(
            parse_status_report(&report[..10]),
            Err(ProtocolError::BadLength(10))
        );
    }

    #[test]
    fn rejects_protocol_version_mismatch() {
        let mut report = sample_status();
        report[6] = PROTOCOL_MINOR + 1;
        assert_eq!(
            parse_status_report(&report),
            Err(ProtocolError::VersionMismatch {
                major: PROTOCOL_MAJOR,
                minor: PROTOCOL_MINOR + 1
            })
        );
    }

    #[test]
    fn parses_ack_report() {
        let mut report = magic_header(REPORT_ID_ACK);
        report[7] = COMMAND_ID_SLEEP_CONTROLLER;
        report[8] = 9;
        report[9] = 0x06; // ErrNotConnected
        report[10] = 0;
        report[11..13].copy_from_slice(&7u16.to_le_bytes());
        report[13..17].copy_from_slice(&1234u32.to_le_bytes());

        let ack = parse_ack_report(&report).expect("ack parses");
        assert_eq!(ack.command_id, COMMAND_ID_SLEEP_CONTROLLER);
        assert_eq!(ack.sequence, 9);
        assert_eq!(ack.result, AckResult::ErrNotConnected);
        assert_eq!(ack.settings_revision, 7);
        assert_eq!(ack.uptime_seconds, 1234);
        assert!(!ack.result.is_ok());
        assert_eq!(ack.result.user_message(), "Controller not connected");
    }

    #[test]
    fn builds_command_report_layout() {
        let report = build_command_report(COMMAND_ID_SET_HOST_PERSONA, 0x42, 0x0207, &[0xAA, 0xBB]);

        assert_eq!(report[0], REPORT_ID_COMMAND);
        assert_eq!(&report[1..5], &MAGIC);
        assert_eq!(report[5], PROTOCOL_MAJOR);
        assert_eq!(report[6], PROTOCOL_MINOR);
        assert_eq!(report[7], COMMAND_ID_SET_HOST_PERSONA);
        assert_eq!(report[8], 0x42);
        assert_eq!(report[9], 0x07);
        assert_eq!(report[10], 0x02);
        assert_eq!(report[11], 0xAA);
        assert_eq!(report[12], 0xBB);
        assert_eq!(report.len(), REPORT_LENGTH);
    }

    #[test]
    fn clamps_idle_disconnect_timeout() {
        assert_eq!(clamp_idle_disconnect_timeout(0), MIN_IDLE_DISCONNECT_TIMEOUT_MINUTES);
        assert_eq!(clamp_idle_disconnect_timeout(15), 15);
        assert_eq!(
            clamp_idle_disconnect_timeout(999),
            MAX_IDLE_DISCONNECT_TIMEOUT_MINUTES
        );

        let report = build_idle_disconnect_timeout_report(1, 999);
        assert_eq!(report[7], COMMAND_ID_SET_IDLE_DISCONNECT_TIMEOUT);
        assert_eq!(
            u16::from_le_bytes([report[9], report[10]]),
            MAX_IDLE_DISCONNECT_TIMEOUT_MINUTES
        );
    }

    #[test]
    fn sleep_report_has_zero_value() {
        let report = build_sleep_controller_report(3);
        assert_eq!(report[7], COMMAND_ID_SLEEP_CONTROLLER);
        assert_eq!(report[8], 3);
        assert_eq!(u16::from_le_bytes([report[9], report[10]]), 0);
    }

    #[test]
    fn persona_support_mask_follows_protocol_values() {
        let mask = 0x01 | 0x02 | 0x04; // dualsense, xbox, ds4 (no edge)
        assert!(HostPersonaMode::DualSense.is_supported_by(mask));
        assert!(HostPersonaMode::Xbox.is_supported_by(mask));
        assert!(HostPersonaMode::Ds4.is_supported_by(mask));
        assert!(!HostPersonaMode::DualSenseEdge.is_supported_by(mask));
        assert!(HostPersonaMode::DualSenseEdge.is_supported_by(mask | 0x80));
    }
}
