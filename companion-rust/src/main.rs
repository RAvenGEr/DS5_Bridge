#![windows_subsystem = "windows"]

mod config;
mod game_monitor;
mod protocol;
mod transport;
mod tray;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};
use slint::{ComponentHandle, VecModel, SharedString, CloseRequestResponse};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, GetWindowTextLengthW, IsWindowVisible,
    SetForegroundWindow, ShowWindow, SW_HIDE, SW_RESTORE,
};

use crate::config::{CompanionConfig, GameRule};
use crate::protocol::{
    build_bool_report, build_idle_disconnect_timeout_report, build_set_persona_report,
    build_sleep_controller_report, clamp_idle_disconnect_timeout, AckPayload,
    BridgeStatusPayload, HostPersonaMode, REPORT_LENGTH,
};
use crate::transport::{PollResult, SendError, TransportManager};
use crate::tray::{TrayAction, TrayManager};

slint::include_modules!();

struct EnumData {
    target_pid: u32,
    found_hwnd: HWND,
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: isize) -> i32 {
    let data = &mut *(lparam as *mut EnumData);
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, &mut pid);
    // Helper windows owned by this process (GPU pbuffer dummies, winit's thread
    // event target, the tray icon window) have an empty title, and the IME
    // windows that do have one are never visible, so the real application
    // window is the only candidate that passes both checks.
    let titled = GetWindowTextLengthW(hwnd) > 0;
    let visible = IsWindowVisible(hwnd) != 0;
    if pid == data.target_pid && titled && visible {
        data.found_hwnd = hwnd;
        return 0; // Stop enumeration
    }
    1 // Continue
}

/// Ask Slint for the native handle of its own window. This is the only
/// authoritative source: enumerating top-level windows can pick up helper
/// windows created by the GPU driver or the windowing backend.
fn hwnd_from_slint(window: &AppWindow) -> Option<HWND> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let slint_window = window.window();
    let handle = slint_window.window_handle();
    match handle.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(win32) => Some(win32.hwnd.get() as HWND),
        _ => None,
    }
}

/// Last-resort lookup used only if Slint cannot expose a raw handle yet. It
/// only ever returns a visible, titled window, and [`resolve_hwnd`] keeps
/// retrying every poll tick until one exists.
fn find_main_window_hwnd() -> Option<HWND> {
    unsafe {
        let mut data = EnumData {
            target_pid: GetCurrentProcessId(),
            found_hwnd: std::ptr::null_mut(),
        };

        EnumWindows(Some(enum_windows_proc), &mut data as *mut EnumData as isize);

        if data.found_hwnd.is_null() {
            None
        } else {
            Some(data.found_hwnd)
        }
    }
}

/// Resolves (and caches) the HWND of the main window. Resolution is deferred
/// until the event loop is running because the native window may not exist yet
/// while `show()` is still being processed.
fn resolve_hwnd(cell: &Cell<HWND>, window: &AppWindow) -> HWND {
    let cached = cell.get();
    if !cached.is_null() {
        return cached;
    }

    match hwnd_from_slint(window).or_else(find_main_window_hwnd) {
        Some(hwnd) => {
            cell.set(hwnd);
            hwnd
        }
        None => std::ptr::null_mut(),
    }
}

fn hide_window(hwnd: HWND) {
    unsafe {
        ShowWindow(hwnd, SW_HIDE);
    }
}

fn show_window(hwnd: HWND) {
    unsafe {
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
    }
}

fn toggle_window(hwnd: HWND) {
    unsafe {
        if IsWindowVisible(hwnd) != 0 {
            hide_window(hwnd);
        } else {
            show_window(hwnd);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let window = AppWindow::new()?;
    let _ = window.show(); // Ensure window is realized by OS

    let hwnd_cell: Rc<Cell<HWND>> = Rc::new(Cell::new(std::ptr::null_mut()));

    let config = Rc::new(RefCell::new(CompanionConfig::load()));
    let transport = Rc::new(RefCell::new(TransportManager::new()));
    let sequence = Rc::new(AtomicU8::new(1));
    let last_auto_switched_game = Rc::new(RefCell::new(String::new()));
    let last_toggle_time = Rc::new(RefCell::new(Instant::now() - Duration::from_secs(5)));
    // Last status report seen from the bridge; used to restore the power
    // settings UI when a command is rejected.
    let last_status: Rc<RefCell<Option<BridgeStatusPayload>>> = Rc::new(RefCell::new(None));
    // Last idle-disconnect timeout pushed into the LineEdit, so status polls
    // never overwrite what the user is currently typing.
    let pushed_timeout: Rc<Cell<Option<u16>>> = Rc::new(Cell::new(None));

    // System Tray initialization
    let tray = match TrayManager::new() {
        Ok(t) => Some(t),
        Err(e) => {
            eprintln!("Failed to initialize system tray: {}", e);
            None
        }
    };

    // Intercept window close button ('X') to hide natively via Win32 API without destroying Slint loop
    {
        let hwnd_cell = hwnd_cell.clone();
        let window_weak = window.as_weak();
        window.window().on_close_requested(move || {
            if let Some(ui) = window_weak.upgrade() {
                let hwnd = resolve_hwnd(&hwnd_cell, &ui);
                if !hwnd.is_null() {
                    hide_window(hwnd);
                }
            }
            CloseRequestResponse::KeepWindowShown
        });
    }

    // Populate Slint UI with initial config data
    {
        let cfg = config.borrow();
        window.set_auto_switch_enabled(cfg.auto_switch_enabled);
        update_game_rules_ui(&window, &cfg.game_rules);
    }

    // Callback: Quick Persona Mode Switch from Slint UI
    {
        let transport = transport.clone();
        let sequence = sequence.clone();
        let window_weak = window.as_weak();
        window.on_set_persona(move |mode_str| {
            let mode = HostPersonaMode::parse_str(&mode_str);
            let outcome =
                send_bridge_command(&transport, &sequence, |seq| build_set_persona_report(seq, mode));

            if let Some(ui) = window_weak.upgrade() {
                let message = format!("Switched to {}", mode.display_name());
                if report_command_outcome(&ui, &outcome, &message) {
                    ui.set_active_persona(SharedString::from(mode.as_str()));
                }
            }
        });
    }

    // Callback: Turn the paired controller off (SLEEP_CONTROLLER)
    {
        let transport = transport.clone();
        let sequence = sequence.clone();
        let window_weak = window.as_weak();
        window.on_sleep_controller(move || {
            let outcome =
                send_bridge_command(&transport, &sequence, build_sleep_controller_report);

            if let Some(ui) = window_weak.upgrade() {
                report_command_outcome(&ui, &outcome, "Controller is going to sleep");
            }
        });
    }

    // Callback: Power settings toggles (idle disconnect, USB suspend, wake on
    // connect, sleep keybind)
    {
        let transport = transport.clone();
        let sequence = sequence.clone();
        let window_weak = window.as_weak();
        let last_status = last_status.clone();
        let pushed_timeout = pushed_timeout.clone();
        window.on_toggle_power_setting(move |name, enabled| {
            let name = name.to_string();
            let command_id = match power_setting_command(&name) {
                Some(command_id) => command_id,
                None => return,
            };

            let outcome =
                send_bridge_command(&transport, &sequence, move |seq| {
                    build_bool_report(command_id, seq, enabled)
                });

            if let Some(ui) = window_weak.upgrade() {
                let message = format!(
                    "{} {}",
                    power_setting_label(&name),
                    if enabled { "enabled" } else { "disabled" }
                );
                if report_command_outcome(&ui, &outcome, &message) {
                    set_power_setting_ui(&ui, &name, enabled);
                } else {
                    restore_power_status(&ui, &last_status, &pushed_timeout);
                }
            }
        });
    }

    // Callback: Idle disconnect timeout (minutes, 1-120)
    {
        let transport = transport.clone();
        let sequence = sequence.clone();
        let window_weak = window.as_weak();
        let last_status = last_status.clone();
        let pushed_timeout = pushed_timeout.clone();
        window.on_set_idle_timeout(move |text| {
            let minutes = match text.trim().parse::<u16>() {
                Ok(value) => clamp_idle_disconnect_timeout(value),
                Err(_) => {
                    if let Some(ui) = window_weak.upgrade() {
                        set_command_status(
                            &ui,
                            false,
                            "Enter a timeout between 1 and 120 minutes",
                        );
                        // Force the field back to the device value even if it
                        // did not change since the last push.
                        pushed_timeout.set(None);
                        restore_power_status(&ui, &last_status, &pushed_timeout);
                    }
                    return;
                }
            };

            let outcome = send_bridge_command(&transport, &sequence, move |seq| {
                build_idle_disconnect_timeout_report(seq, minutes)
            });

            if let Some(ui) = window_weak.upgrade() {
                let message = format!("Idle disconnect set to {} min", minutes);
                if report_command_outcome(&ui, &outcome, &message) {
                    ui.set_idle_timeout_text(SharedString::from(minutes.to_string()));
                    pushed_timeout.set(Some(minutes));
                } else {
                    pushed_timeout.set(None);
                    restore_power_status(&ui, &last_status, &pushed_timeout);
                }
            }
        });
    }

    // Callback: Toggle Auto-Switching
    {
        let config = config.clone();
        window.on_toggle_auto_switch(move |enabled| {
            let mut cfg = config.borrow_mut();
            cfg.auto_switch_enabled = enabled;
            cfg.save();
        });
    }

    // Callback: Add Game Rule
    {
        let config = config.clone();
        let window_weak = window.as_weak();
        window.on_add_game_rule(move |proc_name, mode_str| {
            let proc_clean = proc_name.trim();
            if proc_clean.is_empty() {
                return;
            }

            let mode = HostPersonaMode::parse_str(&mode_str);
            let mut cfg = config.borrow_mut();
            let new_rule = GameRule {
                id: format!("rule-{}", cfg.game_rules.len() + 1),
                process_name: proc_clean.to_string(),
                mode,
            };
            cfg.game_rules.push(new_rule);
            cfg.save();

            if let Some(ui) = window_weak.upgrade() {
                update_game_rules_ui(&ui, &cfg.game_rules);
            }
        });
    }

    // Callback: Remove Game Rule
    {
        let config = config.clone();
        let window_weak = window.as_weak();
        window.on_remove_game_rule(move |rule_id| {
            let mut cfg = config.borrow_mut();
            cfg.game_rules.retain(|r| rule_id != r.id);
            cfg.save();

            if let Some(ui) = window_weak.upgrade() {
                update_game_rules_ui(&ui, &cfg.game_rules);
            }
        });
    }

    // Callback: Minimize to Tray button in UI
    {
        let hwnd_cell = hwnd_cell.clone();
        let window_weak = window.as_weak();
        window.on_minimize_to_tray(move || {
            if let Some(ui) = window_weak.upgrade() {
                let hwnd = resolve_hwnd(&hwnd_cell, &ui);
                if !hwnd.is_null() {
                    hide_window(hwnd);
                }
            }
        });
    }

    // Periodic Timer for Status Polling, System Tray Events, and Game Auto-Switching
    let poll_timer = slint::Timer::default();
    {
        let window_weak = window.as_weak();
        let transport = transport.clone();
        let config = config.clone();
        let sequence = sequence.clone();
        let last_auto_switched_game = last_auto_switched_game.clone();
        let last_toggle_time = last_toggle_time.clone();
        let hwnd_cell = hwnd_cell.clone();
        let last_status = last_status.clone();
        let pushed_timeout = pushed_timeout.clone();
        let mut poll_counter = 0u64;

        poll_timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(150), move || {
            poll_counter += 1;

            let ui = match window_weak.upgrade() {
                Some(ui) => ui,
                None => return,
            };

            // First tick runs once the event loop is up, which is when the
            // native window handle is guaranteed to exist.
            let hwnd = resolve_hwnd(&hwnd_cell, &ui);

            // 1. Poll System Tray Menu & Mouse Events
            if let Some(tray_mgr) = &tray {
                if let Some(action) = tray_mgr.poll_menu_events() {
                    match action {
                        TrayAction::SetPersona(mode_str) => {
                            let mode = HostPersonaMode::parse_str(mode_str);
                            let outcome = send_bridge_command(&transport, &sequence, |seq| {
                                build_set_persona_report(seq, mode)
                            });
                            let message = format!("Switched to {}", mode.display_name());
                            if report_command_outcome(&ui, &outcome, &message) {
                                ui.set_active_persona(SharedString::from(mode.as_str()));
                            }
                        }
                        TrayAction::ToggleWindow => {
                            let mut last_t = last_toggle_time.borrow_mut();
                            if last_t.elapsed() > Duration::from_millis(300) {
                                *last_t = Instant::now();
                                if !hwnd.is_null() {
                                    toggle_window(hwnd);
                                }
                            }
                        }
                        TrayAction::Exit => {
                            let _ = slint::quit_event_loop();
                        }
                    }
                }
            }

            // 2. Poll Hardware Status every ~600ms (every 4 ticks of 150ms)
            if poll_counter % 4 == 0 {
                let poll = transport.borrow_mut().poll_status();
                match poll {
                    PollResult::Status(status) => {
                        ui.set_is_connected(true);
                        ui.set_connection_status_text(SharedString::from("Bridge Connected (USB)"));
                        ui.set_controller_connected(status.controller_connected);
                        ui.set_controller_name(SharedString::from(status.controller_type.as_str()));
                        ui.set_battery_percent(status.battery_percent.map(|b| b as i32).unwrap_or(-1));
                        ui.set_active_persona(SharedString::from(status.host_persona_mode.as_str()));
                        ui.set_firmware_version(SharedString::from(status.firmware_version.as_str()));

                        let mins = status.uptime_seconds / 60;
                        let hrs = mins / 60;
                        let uptime_fmt = if hrs > 0 {
                            format!("{}h {}m", hrs, mins % 60)
                        } else {
                            format!("{}m", mins)
                        };
                        ui.set_uptime_text(SharedString::from(uptime_fmt.as_str()));

                        apply_power_status(&ui, &status, &pushed_timeout);
                        *last_status.borrow_mut() = Some(status);
                    }
                    PollResult::Incompatible { major, minor } => {
                        ui.set_is_connected(true);
                        ui.set_connection_status_text(SharedString::from(format!(
                            "Firmware update required ({}.{})",
                            major, minor
                        )));
                        ui.set_controller_connected(false);
                    }
                    PollResult::Disconnected => {
                        ui.set_is_connected(false);
                        ui.set_connection_status_text(SharedString::from("Searching for DS5 Bridge..."));
                        ui.set_controller_connected(false);
                    }
                }
            }

            // 3. Poll Active Foreground Game Process every ~1.5 seconds (every 10 ticks of 150ms)
            if poll_counter % 10 == 0 {
                if let Some(proc_name) = game_monitor::get_foreground_process_name() {
                    ui.set_detected_game(SharedString::from(proc_name.as_str()));

                    let cfg = config.borrow();
                    if cfg.auto_switch_enabled {
                        let proc_lower = proc_name.to_lowercase();
                        if let Some(rule) = cfg.game_rules.iter().find(|r| r.process_name.to_lowercase() == proc_lower) {
                            ui.set_active_rule_status(SharedString::from(format!("Rule Matched: -> {}", rule.mode.display_name())));

                            let current_persona_str = ui.get_active_persona();
                            let current_persona = HostPersonaMode::parse_str(&current_persona_str);

                            let mut last_game = last_auto_switched_game.borrow_mut();
                            if current_persona != rule.mode || *last_game != proc_name {
                                *last_game = proc_name.clone();
                                // Background loop: retry until the bridge is
                                // reachable, but only announce successes so the
                                // status line is not spammed while disconnected.
                                let outcome = send_bridge_command(&transport, &sequence, |seq| {
                                    build_set_persona_report(seq, rule.mode)
                                });
                                if matches!(&outcome, Ok(ack) if ack.result.is_ok()) {
                                    ui.set_active_persona(SharedString::from(rule.mode.as_str()));
                                    set_command_status(
                                        &ui,
                                        true,
                                        &format!("Auto-switched to {}", rule.mode.display_name()),
                                    );
                                }
                            }
                        } else {
                            ui.set_active_rule_status(SharedString::from("No rule for process"));
                        }
                    } else {
                        ui.set_active_rule_status(SharedString::from("Auto-Switching Disabled"));
                    }
                } else {
                    ui.set_detected_game(SharedString::from("Desktop / None"));
                    ui.set_active_rule_status(SharedString::from("Idle"));
                }
            }
        });
    }

    window.run()?;
    Ok(())
}

/// Builds a command report and sends it, waiting for the bridge's ACK.
fn send_bridge_command(
    transport: &Rc<RefCell<TransportManager>>,
    sequence: &Rc<AtomicU8>,
    build: impl FnOnce(u8) -> [u8; REPORT_LENGTH],
) -> Result<AckPayload, SendError> {
    let sequence_number = sequence.fetch_add(1, Ordering::SeqCst);
    let report = build(sequence_number);
    transport.borrow_mut().send_command(&report)
}

fn set_command_status(ui: &AppWindow, ok: bool, message: &str) {
    ui.set_command_status_ok(ok);
    ui.set_command_status_text(SharedString::from(message));
}

/// Mirrors a command outcome onto the status line. Returns `true` when the
/// bridge accepted the command.
fn report_command_outcome(
    ui: &AppWindow,
    outcome: &Result<AckPayload, SendError>,
    success_message: &str,
) -> bool {
    match outcome {
        Ok(ack) if ack.result.is_ok() => {
            set_command_status(ui, true, success_message);
            true
        }
        Ok(ack) => {
            set_command_status(ui, false, &ack.result.user_message());
            false
        }
        Err(error) => {
            set_command_status(ui, false, &error.user_message());
            false
        }
    }
}

fn power_setting_command(name: &str) -> Option<u8> {
    use crate::protocol::{
        COMMAND_ID_SET_IDLE_DISCONNECT_ENABLED, COMMAND_ID_SET_SLEEP_KEYBIND_ENABLED,
        COMMAND_ID_SET_USB_SUSPEND_DISCONNECT_ENABLED, COMMAND_ID_SET_WAKE_ON_CONNECT,
    };
    Some(match name {
        "idle_disconnect" => COMMAND_ID_SET_IDLE_DISCONNECT_ENABLED,
        "usb_suspend_disconnect" => COMMAND_ID_SET_USB_SUSPEND_DISCONNECT_ENABLED,
        "wake_on_connect" => COMMAND_ID_SET_WAKE_ON_CONNECT,
        "sleep_keybind" => COMMAND_ID_SET_SLEEP_KEYBIND_ENABLED,
        _ => return None,
    })
}

fn power_setting_label(name: &str) -> &'static str {
    match name {
        "idle_disconnect" => "Idle disconnect",
        "usb_suspend_disconnect" => "USB suspend disconnect",
        "wake_on_connect" => "Wake on connect",
        "sleep_keybind" => "Sleep keybind",
        _ => "Setting",
    }
}

/// Updates the UI property backing one power setting.
fn set_power_setting_ui(ui: &AppWindow, name: &str, enabled: bool) {
    match name {
        "idle_disconnect" => ui.set_idle_disconnect_enabled(enabled),
        "usb_suspend_disconnect" => ui.set_usb_suspend_disconnect_enabled(enabled),
        "wake_on_connect" => ui.set_wake_on_connect_enabled(enabled),
        "sleep_keybind" => ui.set_sleep_keybind_enabled(enabled),
        _ => {}
    }
}

/// Pushes the bridge's power settings into the UI. The idle-timeout field is
/// only rewritten when the device value actually changes, so status polls never
/// overwrite what the user is typing.
fn apply_power_status(
    ui: &AppWindow,
    status: &BridgeStatusPayload,
    pushed_timeout: &Cell<Option<u16>>,
) {
    ui.set_idle_disconnect_enabled(status.idle_disconnect_enabled);
    ui.set_usb_suspend_disconnect_enabled(status.usb_suspend_disconnect_enabled);
    ui.set_wake_on_connect_enabled(status.wake_on_connect_enabled);
    ui.set_sleep_keybind_enabled(status.sleep_keybind_enabled);

    let minutes = status.idle_disconnect_timeout_minutes;
    if pushed_timeout.get() != Some(minutes) {
        ui.set_idle_timeout_text(SharedString::from(minutes.to_string()));
        pushed_timeout.set(Some(minutes));
    }
}

/// Puts the power settings UI back in line with the last known device state,
/// used whenever a command was rejected or could not be delivered.
fn restore_power_status(
    ui: &AppWindow,
    last_status: &Rc<RefCell<Option<BridgeStatusPayload>>>,
    pushed_timeout: &Rc<Cell<Option<u16>>>,
) {
    if let Some(status) = last_status.borrow().as_ref() {
        apply_power_status(ui, status, pushed_timeout);
    }
}

fn update_game_rules_ui(window: &AppWindow, rules: &[GameRule]) {
    let slint_rules: Vec<GameRuleData> = rules
        .iter()
        .map(|r| GameRuleData {
            id: SharedString::from(r.id.as_str()),
            process_name: SharedString::from(r.process_name.as_str()),
            mode: SharedString::from(r.mode.as_str()),
        })
        .collect();

    let model = Rc::new(VecModel::from(slint_rules));
    window.set_game_rules(model.into());
}
