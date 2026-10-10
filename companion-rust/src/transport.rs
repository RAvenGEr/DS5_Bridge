use std::ptr::null_mut;
use std::ffi::c_void;
use std::sync::OnceLock;
use std::time::Duration;
use crate::protocol::{
    parse_ack_report, parse_status_report, AckPayload, BridgeStatusPayload, ProtocolError,
    REPORT_ID_ACK, REPORT_LENGTH,
};

pub const DEVICE_INTERFACE_GUID_PRIMARY: [u8; 16] = [
    0xa9, 0xb2, 0xc8, 0xe4, 0xf5, 0x87, 0x4c, 0x4c, 0x9e, 0x52, 0x2b, 0x4c, 0x1b, 0x8b, 0x4f, 0x62
];
pub const DEVICE_INTERFACE_GUID_LEGACY: [u8; 16] = [
    0xf4, 0xc5, 0xb7, 0xd5, 0x68, 0x8a, 0x86, 0x4a, 0x9e, 0x31, 0x1e, 0x5f, 0xa7, 0xb1, 0xd5, 0xb0
];

pub const GENERIC_READ: u32 = 0x80000000;
pub const GENERIC_WRITE: u32 = 0x40000000;

/// Same bulk OUT timeout the .NET AudioHelper applies (`BridgeOutTransferTimeoutMs`).
const BRIDGE_OUT_TRANSFER_TIMEOUT_MS: u32 = 35;
/// Control transfers are more expensive; give the firmware a full second.
const CONTROL_TRANSFER_TIMEOUT_MS: u32 = 1000;
const PIPE_TRANSFER_TIMEOUT: u32 = 0x03;
const ERROR_SEM_TIMEOUT: u32 = 121;
/// Number of ACK read attempts before giving up on a command.
const ACK_READ_ATTEMPTS: u32 = 3;
const ACK_READ_RETRY_DELAY: Duration = Duration::from_millis(15);

/// Outcome of a status poll: the bridge is either healthy, running firmware
/// that speaks a different protocol revision, or unreachable.
#[derive(Debug, Clone, PartialEq)]
pub enum PollResult {
    Status(BridgeStatusPayload),
    Incompatible { major: u8, minor: u8 },
    Disconnected,
}

/// Why a command could not be delivered/confirmed. The command itself may
/// still have been rejected by the firmware; that shows up as
/// `Ok(ack)` with a non-OK `ack.result`.
#[derive(Debug, Clone, PartialEq)]
pub enum SendError {
    NotConnected,
    WriteFailed,
    NoAck,
    VersionMismatch { major: u8, minor: u8 },
    StaleAck {
        expected_command: u8,
        expected_sequence: u8,
        actual_command: u8,
        actual_sequence: u8,
    },
}

impl SendError {
    pub fn user_message(&self) -> String {
        match self {
            SendError::NotConnected => "DS5 Bridge is not connected".to_string(),
            SendError::WriteFailed => "Could not write to the DS5 Bridge".to_string(),
            SendError::NoAck => "The DS5 Bridge did not confirm the command".to_string(),
            SendError::VersionMismatch { major, minor } => format!(
                "Firmware update required (bridge speaks {}.{}).",
                major, minor
            ),
            SendError::StaleAck {
                expected_command,
                expected_sequence,
                actual_command,
                actual_sequence,
            } => format!(
                "Stale ACK (expected 0x{:02x} seq {}, got 0x{:02x} seq {})",
                expected_command, expected_sequence, actual_command, actual_sequence
            ),
        }
    }
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct WinUsbSetupPacket {
    pub request_type: u8,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub length: u16,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct WinUsbPipeInformation {
    pub pipe_type: u32, // 2 = Bulk
    pub pipe_id: u8,
    pub interval: u8,
    pub maximum_packet_size: u16,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct UsbInterfaceDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub b_interface_number: u8,
    pub b_alternate_setting: u8,
    pub b_num_endpoints: u8,
    pub b_interface_class: u8,
    pub b_interface_sub_class: u8,
    pub b_interface_protocol: u8,
    pub i_interface: u8,
}

type WinUsbHandle = *mut c_void;

#[cfg(target_os = "windows")]
mod win32 {
    use super::*;
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ,
        FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
        SetupDiGetDeviceInterfaceDetailW, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
        SP_DEVICE_INTERFACE_DATA, SP_DEVINFO_DATA, HDEVINFO,
    };
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};

    type FnWinUsbInitialize = unsafe extern "system" fn(HANDLE, *mut WinUsbHandle) -> i32;
    type FnWinUsbFree = unsafe extern "system" fn(WinUsbHandle) -> i32;
    type FnWinUsbControlTransfer = unsafe extern "system" fn(
        WinUsbHandle,
        WinUsbSetupPacket,
        *mut u8,
        u32,
        *mut u32,
        *const c_void,
    ) -> i32;
    type FnWinUsbQueryInterfaceSettings = unsafe extern "system" fn(
        WinUsbHandle,
        u8,
        *mut UsbInterfaceDescriptor,
    ) -> i32;
    type FnWinUsbQueryPipe = unsafe extern "system" fn(
        WinUsbHandle,
        u8,
        u8,
        *mut WinUsbPipeInformation,
    ) -> i32;
    type FnWinUsbWritePipe = unsafe extern "system" fn(
        WinUsbHandle,
        u8,
        *const u8,
        u32,
        *mut u32,
        *const c_void,
    ) -> i32;
    type FnWinUsbSetPipePolicy = unsafe extern "system" fn(
        WinUsbHandle,
        u8,
        u32,
        u32,
        *const c_void,
    ) -> i32;

    pub struct WinUsbFns {
        pub initialize: FnWinUsbInitialize,
        pub free: FnWinUsbFree,
        pub control_transfer: FnWinUsbControlTransfer,
        pub query_interface: FnWinUsbQueryInterfaceSettings,
        pub query_pipe: FnWinUsbQueryPipe,
        pub write_pipe: FnWinUsbWritePipe,
        pub set_pipe_policy: Option<FnWinUsbSetPipePolicy>,
    }

    unsafe impl Send for WinUsbFns {}
    unsafe impl Sync for WinUsbFns {}

    static WINUSB_FNS: OnceLock<Option<WinUsbFns>> = OnceLock::new();

    fn get_winusb_fns() -> Option<&'static WinUsbFns> {
        WINUSB_FNS.get_or_init(|| unsafe {
            let lib = LoadLibraryA(b"winusb.dll\0".as_ptr());
            if lib.is_null() {
                return None;
            }
            let init_ptr = GetProcAddress(lib, b"WinUsb_Initialize\0".as_ptr());
            let free_ptr = GetProcAddress(lib, b"WinUsb_Free\0".as_ptr());
            let ctrl_ptr = GetProcAddress(lib, b"WinUsb_ControlTransfer\0".as_ptr());
            let qif_ptr = GetProcAddress(lib, b"WinUsb_QueryInterfaceSettings\0".as_ptr());
            let qpipe_ptr = GetProcAddress(lib, b"WinUsb_QueryPipe\0".as_ptr());
            let write_ptr = GetProcAddress(lib, b"WinUsb_WritePipe\0".as_ptr());
            let policy_ptr = GetProcAddress(lib, b"WinUsb_SetPipePolicy\0".as_ptr());

            if let (Some(init), Some(free), Some(ctrl), Some(qif), Some(qpipe), Some(write)) = (
                init_ptr.map(|p| std::mem::transmute(p)),
                free_ptr.map(|p| std::mem::transmute(p)),
                ctrl_ptr.map(|p| std::mem::transmute(p)),
                qif_ptr.map(|p| std::mem::transmute(p)),
                qpipe_ptr.map(|p| std::mem::transmute(p)),
                write_ptr.map(|p| std::mem::transmute(p)),
            ) {
                Some(WinUsbFns {
                    initialize: init,
                    free,
                    control_transfer: ctrl,
                    query_interface: qif,
                    query_pipe: qpipe,
                    write_pipe: write,
                    set_pipe_policy: policy_ptr.map(|p| std::mem::transmute(p)),
                })
            } else {
                None
            }
        }).as_ref()
    }

    pub struct WinUsbTransport {
        file_handle: HANDLE,
        winusb_handle: WinUsbHandle,
        out_pipe_id: Option<u8>,
        interface_number: u16,
    }

    impl WinUsbTransport {
        pub fn open() -> Option<Self> {
            let guids = [DEVICE_INTERFACE_GUID_PRIMARY, DEVICE_INTERFACE_GUID_LEGACY];
            for guid_bytes in guids {
                if let Some(transport) = Self::open_by_guid(&guid_bytes) {
                    return Some(transport);
                }
            }
            None
        }

        fn open_by_guid(guid_bytes: &[u8; 16]) -> Option<Self> {
            let winusb = get_winusb_fns()?;
            unsafe {
                let dev_info: HDEVINFO = SetupDiGetClassDevsW(
                    guid_bytes.as_ptr() as *const _,
                    null_mut(),
                    null_mut(),
                    DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
                );

                let invalid_dev_info = INVALID_HANDLE_VALUE as HDEVINFO;
                if dev_info == invalid_dev_info || dev_info == 0 as HDEVINFO {
                    return None;
                }

                let mut interface_data: SP_DEVICE_INTERFACE_DATA = std::mem::zeroed();
                interface_data.cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;

                let mut index = 0;
                while SetupDiEnumDeviceInterfaces(dev_info, null_mut(), guid_bytes.as_ptr() as *const _, index, &mut interface_data) != 0 {
                    index += 1;

                    let mut req_size = 0;
                    SetupDiGetDeviceInterfaceDetailW(dev_info, &interface_data, null_mut(), 0, &mut req_size, null_mut());

                    if req_size == 0 {
                        continue;
                    }

                    let mut buf = vec![0u8; req_size as usize];
                    let cb_size = if std::mem::size_of::<usize>() == 8 { 8 } else { 6 };
                    *(buf.as_mut_ptr() as *mut u32) = cb_size;

                    let mut dev_data: SP_DEVINFO_DATA = std::mem::zeroed();
                    dev_data.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;

                    if SetupDiGetDeviceInterfaceDetailW(
                        dev_info,
                        &interface_data,
                        buf.as_mut_ptr() as *mut _,
                        req_size,
                        null_mut(),
                        &mut dev_data,
                    ) != 0 {
                        let path_ptr = buf.as_ptr().add(4) as *const u16;
                        let mut len = 0;
                        while *path_ptr.add(len) != 0 {
                            len += 1;
                        }
                        let device_path_w: Vec<u16> = std::slice::from_raw_parts(path_ptr, len + 1).to_vec();
                        let device_path_str = String::from_utf16_lossy(&device_path_w[..len]);

                        let mut interface_number = 0u16;
                        if let Some(mi_pos) = device_path_str.to_lowercase().find("mi_") {
                            if device_path_str.len() >= mi_pos + 5 {
                                if let Ok(num) = u16::from_str_radix(&device_path_str[mi_pos + 3..mi_pos + 5], 16) {
                                    interface_number = num;
                                }
                            }
                        }

                        let file_handle = CreateFileW(
                            device_path_w.as_ptr(),
                            GENERIC_READ | GENERIC_WRITE,
                            FILE_SHARE_READ | FILE_SHARE_WRITE,
                            null_mut(),
                            OPEN_EXISTING,
                            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED,
                            null_mut(),
                        );

                        if file_handle != INVALID_HANDLE_VALUE && !file_handle.is_null() {
                            let mut winusb_handle: WinUsbHandle = null_mut();
                            if (winusb.initialize)(file_handle, &mut winusb_handle) != 0 {
                                let mut out_pipe_id = None;
                                let mut if_desc: UsbInterfaceDescriptor = std::mem::zeroed();
                                if (winusb.query_interface)(winusb_handle, 0, &mut if_desc) != 0 {
                                    for ep_idx in 0..if_desc.b_num_endpoints {
                                        let mut pipe_info: WinUsbPipeInformation = std::mem::zeroed();
                                        if (winusb.query_pipe)(winusb_handle, 0, ep_idx, &mut pipe_info) != 0 {
                                            if pipe_info.pipe_type == 2 && (pipe_info.pipe_id & 0x80) == 0 {
                                                out_pipe_id = Some(pipe_info.pipe_id);
                                                break;
                                            }
                                        }
                                    }
                                }

                                // Match the .NET helper: bound bulk OUT transfers
                                // so a wedged device cannot hang the UI thread.
                                if let (Some(policy), Some(pipe_id)) =
                                    (winusb.set_pipe_policy, out_pipe_id)
                                {
                                    let out_timeout = BRIDGE_OUT_TRANSFER_TIMEOUT_MS;
                                    (policy)(
                                        winusb_handle,
                                        pipe_id,
                                        PIPE_TRANSFER_TIMEOUT,
                                        std::mem::size_of::<u32>() as u32,
                                        &out_timeout as *const u32 as *const c_void,
                                    );
                                }
                                if let Some(policy) = winusb.set_pipe_policy {
                                    let control_timeout = CONTROL_TRANSFER_TIMEOUT_MS;
                                    // Pipe 0 is the default control pipe; older
                                    // WinUSB builds reject this, which is fine.
                                    (policy)(
                                        winusb_handle,
                                        0,
                                        PIPE_TRANSFER_TIMEOUT,
                                        std::mem::size_of::<u32>() as u32,
                                        &control_timeout as *const u32 as *const c_void,
                                    );
                                }

                                SetupDiDestroyDeviceInfoList(dev_info);
                                return Some(WinUsbTransport {
                                    file_handle,
                                    winusb_handle,
                                    out_pipe_id,
                                    interface_number,
                                });
                            }
                            CloseHandle(file_handle);
                        }
                    }
                }

                SetupDiDestroyDeviceInfoList(dev_info);
            }
            None
        }

        /// Reads any companion feature report (status `0x01`, ACK `0x03`, ...)
        /// through the same control transfer the .NET helper uses
        /// (`bmRequestType=0xC1`, `bRequest=GET_REPORT(0x31)`, `wValue=report id`).
        pub fn get_report(&self, report_id: u8) -> Option<[u8; REPORT_LENGTH]> {
            let winusb = get_winusb_fns()?;
            let setup = WinUsbSetupPacket {
                request_type: 0xC1,
                request: 0x31,
                value: report_id as u16,
                index: self.interface_number,
                length: REPORT_LENGTH as u16,
            };

            let mut report = [0u8; REPORT_LENGTH];
            let mut transferred = 0u32;
            unsafe {
                let res = (winusb.control_transfer)(
                    self.winusb_handle,
                    setup,
                    report.as_mut_ptr(),
                    REPORT_LENGTH as u32,
                    &mut transferred,
                    null_mut(),
                );
                if res != 0 && transferred == REPORT_LENGTH as u32 {
                    return Some(report);
                }
            }
            None
        }

        pub fn get_status_report(&self) -> Option<[u8; REPORT_LENGTH]> {
            self.get_report(crate::protocol::REPORT_ID_STATUS)
        }

        pub fn write_report(&self, report: &[u8; REPORT_LENGTH]) -> bool {
            let winusb = match get_winusb_fns() {
                Some(fns) => fns,
                None => return false,
            };

            unsafe {
                if let Some(pipe_id) = self.out_pipe_id {
                    // WinUSB retries a timed-out bulk OUT once, mirroring the
                    // .NET helper (`ERROR_SEM_TIMEOUT` on the first attempt).
                    for attempt in 0..2u32 {
                        let mut transferred = 0u32;
                        let res = (winusb.write_pipe)(
                            self.winusb_handle,
                            pipe_id,
                            report.as_ptr(),
                            REPORT_LENGTH as u32,
                            &mut transferred,
                            null_mut(),
                        );
                        if res != 0 {
                            // A short write never reaches the firmware intact.
                            return transferred == REPORT_LENGTH as u32;
                        }
                        if GetLastError() != ERROR_SEM_TIMEOUT || attempt != 0 {
                            break;
                        }
                    }
                }

                let setup = WinUsbSetupPacket {
                    request_type: 0x41,
                    request: 0x32,
                    value: report[0] as u16,
                    index: self.interface_number,
                    length: REPORT_LENGTH as u16,
                };
                let mut transferred = 0u32;
                let mut buf = *report;
                let res = (winusb.control_transfer)(
                    self.winusb_handle,
                    setup,
                    buf.as_mut_ptr(),
                    REPORT_LENGTH as u32,
                    &mut transferred,
                    null_mut(),
                );
                res != 0 && transferred == REPORT_LENGTH as u32
            }
        }
    }

    impl Drop for WinUsbTransport {
        fn drop(&mut self) {
            if let Some(winusb) = get_winusb_fns() {
                unsafe {
                    if !self.winusb_handle.is_null() {
                        (winusb.free)(self.winusb_handle);
                    }
                    if !self.file_handle.is_null() && self.file_handle != INVALID_HANDLE_VALUE {
                        CloseHandle(self.file_handle);
                    }
                }
            }
        }
    }
}

pub struct TransportManager {
    #[cfg(target_os = "windows")]
    transport: Option<win32::WinUsbTransport>,
}

impl TransportManager {
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "windows")]
            transport: None,
        }
    }

    pub fn poll_status(&mut self) -> PollResult {
        #[cfg(target_os = "windows")]
        {
            if self.transport.is_none() {
                self.transport = win32::WinUsbTransport::open();
            }

            if let Some(transport) = &self.transport {
                if let Some(raw) = transport.get_status_report() {
                    match parse_status_report(&raw) {
                        Ok(status) => return PollResult::Status(status),
                        // Reachable but speaking another protocol revision:
                        // keep the handle so the UI can explain the mismatch.
                        Err(ProtocolError::VersionMismatch { major, minor }) => {
                            return PollResult::Incompatible { major, minor };
                        }
                        Err(_) => {}
                    }
                }
                // The bridge stopped answering; drop the handle so the next
                // poll re-discovers it.
                self.transport = None;
            }
        }
        PollResult::Disconnected
    }

    /// Writes a command report and waits for the matching ACK report, the same
    /// handshake the Electron companion performs over HID feature reports.
    pub fn send_command(&mut self, report: &[u8; REPORT_LENGTH]) -> Result<AckPayload, SendError> {
        #[cfg(target_os = "windows")]
        {
            let write_ok = {
                let transport = match self.transport_or_open() {
                    Some(transport) => transport,
                    None => return Err(SendError::NotConnected),
                };
                transport.write_report(report)
            };

            if !write_ok {
                self.transport = None;
                return Err(SendError::WriteFailed);
            }

            let expected_command = report[7];
            let expected_sequence = report[8];
            let mut stale: Option<(u8, u8)> = None;

            for attempt in 0..ACK_READ_ATTEMPTS {
                if attempt > 0 {
                    std::thread::sleep(ACK_READ_RETRY_DELAY);
                }

                let raw = match self.transport_or_open().and_then(|t| t.get_report(REPORT_ID_ACK)) {
                    Some(raw) => raw,
                    None => continue,
                };

                match parse_ack_report(&raw) {
                    Ok(ack) => {
                        if ack.command_id == expected_command && ack.sequence == expected_sequence {
                            return Ok(ack);
                        }
                        stale = Some((ack.command_id, ack.sequence));
                    }
                    Err(ProtocolError::VersionMismatch { major, minor }) => {
                        return Err(SendError::VersionMismatch { major, minor });
                    }
                    // Unreadable ACK: retry, it may be a stale/garbled report.
                    Err(_) => continue,
                }
            }

            return Err(match stale {
                Some((actual_command, actual_sequence)) => SendError::StaleAck {
                    expected_command,
                    expected_sequence,
                    actual_command,
                    actual_sequence,
                },
                None => SendError::NoAck,
            });
        }

        #[cfg(not(target_os = "windows"))]
        Err(SendError::NotConnected)
    }

    #[cfg(target_os = "windows")]
    fn transport_or_open(&mut self) -> Option<&win32::WinUsbTransport> {
        if self.transport.is_none() {
            self.transport = win32::WinUsbTransport::open();
        }
        self.transport.as_ref()
    }
}
