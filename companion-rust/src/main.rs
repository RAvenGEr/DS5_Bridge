#![windows_subsystem = "windows"]
#![forbid(unsafe_code)]

mod config;
mod game_monitor;
mod tray;

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText, Vec2};
use ds5_transport::{
    build_bool_report, build_idle_disconnect_timeout_report, build_set_persona_report,
    build_sleep_controller_report, build_value_report, clamp_idle_disconnect_timeout, AckPayload,
    AudioPacketMode, BridgeStatusPayload, HostPersonaMode, COMMAND_ID_SET_AUDIO_PACKET_MODE,
    REPORT_LENGTH,
};
use ds5_transport::{PollResult, SendError, TransportManager};

struct CompanionApp {
    config: config::CompanionConfig,
    transport: RefCell<TransportManager>,
    sequence: AtomicU8,
    _tray: Option<tray::TrayManager>,
    tray_actions: mpsc::Receiver<tray::TrayAction>,
    window_visible: Arc<AtomicBool>,
    exit_requested: Arc<AtomicBool>,
    connected: bool,
    connection_text: String,
    controller_connected: bool,
    controller_name: String,
    battery: Option<u8>,
    firmware: String,
    uptime: String,
    persona: HostPersonaMode,
    last_status: Option<BridgeStatusPayload>,
    idle_timeout: u16,
    idle_timeout_input: String,
    idle_timeout_dirty: bool,
    packet_mode: AudioPacketMode,
    packet_mode_supported: bool,
    packet_mode_applied: bool,
    auto_switch_enabled: bool,
    detected_game: String,
    rule_status: String,
    command_text: String,
    command_ok: bool,
    new_process: String,
    next_poll: Instant,
    poll_counter: u64,
    last_auto_game: String,
}

impl CompanionApp {
    fn new(ctx: &egui::Context) -> Self {
        let config = config::CompanionConfig::load();
        let packet_mode = config.audio_packet_mode;
        let auto_switch_enabled = config.auto_switch_enabled;
        let window_visible = Arc::new(AtomicBool::new(true));
        let exit_requested = Arc::new(AtomicBool::new(false));
        let (tray_sender, tray_actions) = mpsc::channel();
        let toggle_visible = window_visible.clone();
        let toggle_ctx = ctx.clone();
        let toggle_debounce = Arc::new(Mutex::new(Instant::now() - Duration::from_secs(2)));
        let toggle_window_callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            if let Ok(mut previous) = toggle_debounce.lock() {
                if previous.elapsed() >= Duration::from_millis(350) {
                    *previous = Instant::now();
                    let visible = !toggle_visible.load(Ordering::Relaxed);
                    toggle_visible.store(visible, Ordering::Relaxed);
                    toggle_ctx.send_viewport_cmd(egui::ViewportCommand::Visible(visible));
                    toggle_ctx.request_repaint();
                }
            }
        });
        let tray = match tray::TrayManager::new(
            tray_sender,
            ctx.clone(),
            toggle_window_callback.clone(),
            {
                let exit_requested = exit_requested.clone();
                let exit_ctx = ctx.clone();
                Arc::new(move || {
                    exit_requested.store(true, Ordering::Relaxed);
                    exit_ctx.request_repaint();
                })
            },
        ) {
            Ok(t) => Some(t),
            Err(e) => {
                eprintln!("Failed to initialize tray: {e}");
                None
            }
        };
        Self {
            config,
            transport: RefCell::new(TransportManager::new()),
            sequence: AtomicU8::new(1),
            _tray: tray,
            tray_actions,
            window_visible,
            exit_requested,
            connected: false,
            connection_text: "Searching for DS5 Bridge…".into(),
            controller_connected: false,
            controller_name: "Controller Disconnected".into(),
            battery: None,
            firmware: "—".into(),
            uptime: "0m".into(),
            persona: HostPersonaMode::DualSense,
            last_status: None,
            idle_timeout: 15,
            idle_timeout_input: "15".into(),
            idle_timeout_dirty: false,
            packet_mode,
            packet_mode_supported: false,
            packet_mode_applied: false,
            auto_switch_enabled,
            detected_game: "Desktop / None".into(),
            rule_status: "Idle".into(),
            command_text: "Ready".into(),
            command_ok: true,
            new_process: String::new(),
            next_poll: Instant::now(),
            poll_counter: 0,
            last_auto_game: String::new(),
        }
    }

    fn command(
        &self,
        build: impl FnOnce(u8) -> [u8; REPORT_LENGTH],
    ) -> Result<AckPayload, SendError> {
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);
        self.transport.borrow_mut().send_command(&build(seq))
    }

    fn outcome(
        &mut self,
        result: Result<AckPayload, SendError>,
        success: impl Into<String>,
    ) -> bool {
        match result {
            Ok(ack) if ack.result.is_ok() => {
                self.command_text = success.into();
                self.command_ok = true;
                true
            }
            Ok(ack) => {
                self.command_text = ack.result.user_message();
                self.command_ok = false;
                false
            }
            Err(err) => {
                self.command_text = err.user_message();
                self.command_ok = false;
                false
            }
        }
    }

    fn switch_persona(&mut self, mode: HostPersonaMode) {
        let result = self.command(|seq| build_set_persona_report(seq, mode));
        if self.outcome(result, format!("Switched to {}", mode.display_name())) {
            self.persona = mode;
        }
    }

    fn set_power(&mut self, name: &'static str, enabled: bool) {
        let id = power_command(name);
        let result = self.command(|seq| build_bool_report(id, seq, enabled));
        let label = power_label(name);
        if self.outcome(
            result,
            format!("{label} {}", if enabled { "enabled" } else { "disabled" }),
        ) {
            if let Some(status) = &mut self.last_status {
                match name {
                    "idle_disconnect" => status.idle_disconnect_enabled = enabled,
                    "usb_suspend_disconnect" => status.usb_suspend_disconnect_enabled = enabled,
                    "wake_on_connect" => status.wake_on_connect_enabled = enabled,
                    "sleep_keybind" => status.sleep_keybind_enabled = enabled,
                    _ => {}
                }
            }
        }
    }

    fn poll(&mut self) {
        while let Ok(action) = self.tray_actions.try_recv() {
            match action {
                tray::TrayAction::SetPersona(mode) => {
                    self.switch_persona(HostPersonaMode::parse_str(mode));
                }
            }
        }
        self.poll_counter += 1;
        if self.poll_counter % 4 == 0 {
            let poll_result = self.transport.borrow_mut().poll_status();
            match poll_result {
                PollResult::Status(s) => {
                    self.connected = true;
                    self.connection_text = "Bridge Connected (USB)".into();
                    self.controller_connected = s.controller_connected;
                    self.controller_name = s.controller_type.as_str().into();
                    self.battery = s.battery_percent;
                    self.persona = s.host_persona_mode;
                    self.firmware = s.firmware_version.clone();
                    let mins = s.uptime_seconds / 60;
                    self.uptime = if mins / 60 > 0 {
                        format!("{}h {}m", mins / 60, mins % 60)
                    } else {
                        format!("{mins}m")
                    };
                    self.packet_mode_supported = s.audio_packet_mode_control;
                    if !self.idle_timeout_dirty {
                        self.idle_timeout = s.idle_disconnect_timeout_minutes;
                        self.idle_timeout_input = self.idle_timeout.to_string();
                    }
                    if s.audio_packet_mode_control && !self.packet_mode_applied {
                        let value = self.config.audio_packet_mode.to_protocol_value();
                        let result = self.command(|seq| {
                            build_value_report(COMMAND_ID_SET_AUDIO_PACKET_MODE, seq, value)
                        });
                        if matches!(result, Ok(ref ack) if ack.result.is_ok()) {
                            self.packet_mode = self.config.audio_packet_mode;
                            self.packet_mode_applied = true;
                        }
                    }
                    self.last_status = Some(s);
                }
                PollResult::Incompatible { major, minor } => {
                    self.packet_mode_applied = false;
                    self.packet_mode_supported = false;
                    self.connected = true;
                    self.connection_text = format!("Firmware update required ({major}.{minor})");
                    self.controller_connected = false;
                }
                PollResult::Disconnected => {
                    self.packet_mode_applied = false;
                    self.packet_mode_supported = false;
                    self.connected = false;
                    self.connection_text = "Searching for DS5 Bridge…".into();
                    self.controller_connected = false;
                }
            }
        }
        if self.poll_counter % 10 == 0 {
            if let Some(process) = game_monitor::get_foreground_process_name() {
                self.detected_game = process.clone();
                if self.auto_switch_enabled {
                    let rule = self
                        .config
                        .game_rules
                        .iter()
                        .find(|r| r.process_name.eq_ignore_ascii_case(&process))
                        .cloned();
                    if let Some(rule) = rule {
                        self.rule_status = format!("Rule matched: {}", rule.mode.display_name());
                        if self.persona != rule.mode || self.last_auto_game != process {
                            self.last_auto_game = process;
                            let result =
                                self.command(|seq| build_set_persona_report(seq, rule.mode));
                            if self.outcome(
                                result,
                                format!("Auto-switched to {}", rule.mode.display_name()),
                            ) {
                                self.persona = rule.mode;
                            }
                        }
                    } else {
                        self.rule_status = "No rule for process".into();
                    }
                } else {
                    self.rule_status = "Auto-switching disabled".into();
                }
            } else {
                self.detected_game = "Desktop / None".into();
                self.rule_status = "Idle".into();
            }
        }
    }
}

impl eframe::App for CompanionApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.exit_requested.load(Ordering::Relaxed) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if ctx.input(|input| input.viewport().close_requested())
            && !self.exit_requested.load(Ordering::Relaxed)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.window_visible.store(false, Ordering::Relaxed);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        if Instant::now() >= self.next_poll {
            self.poll();
            self.next_poll = Instant::now() + Duration::from_millis(150);
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| { ui.heading(RichText::new("DS5 BRIDGE").strong()); ui.label(RichText::new("Rust Companion v0.3.0").small().weak()); });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let color = if self.connected { Color32::from_rgb(40, 180, 130) } else { Color32::from_rgb(200, 80, 80) };
                    ui.colored_label(color, format!("● {}", self.connection_text));
                });
            });
            ui.add_space(10.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(if self.controller_connected { "🎮" } else { "○" });
                    ui.vertical(|ui| { ui.strong(if self.controller_connected { &self.controller_name } else { "Controller Disconnected" }); ui.label(RichText::new(if self.controller_connected { "Connected via Pico Bridge" } else { "Waiting for controller" }).small().weak()); });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.vertical(|ui| { ui.label(format!("FW {}", self.firmware)); ui.label(RichText::new(format!("Uptime: {}", self.uptime)).small().weak()); }); if let Some(b) = self.battery { ui.colored_label(if b < 20 { Color32::LIGHT_RED } else { Color32::LIGHT_GREEN }, format!("▣ {b}%")); } });
                });
            });
            ui.add_space(8.0);
            ui.label(RichText::new("QUICK MODE SWITCHER").color(Color32::from_rgb(115, 130, 255)).strong());
            let card_gap = ui.spacing().item_spacing.x;
            let card_width = ((ui.available_width() - card_gap * 3.0) / 4.0).max(108.0);
            ui.horizontal(|ui| {
                for (mode, platform, title, subtitle, accent) in [
                    (HostPersonaMode::DualSense, "PlayStation 5", "DualSense", "Adaptive triggers", Color32::from_rgb(129, 140, 248)),
                    (HostPersonaMode::DualSenseEdge, "PlayStation 5", "DualSense Edge", "Back buttons", Color32::from_rgb(192, 132, 252)),
                    (HostPersonaMode::Ds4, "PlayStation 4", "DualShock 4", "Remote Play", Color32::from_rgb(56, 189, 248)),
                    (HostPersonaMode::Xbox, "Windows PC", "Xbox 360", "Native XInput", Color32::from_rgb(74, 222, 128)),
                ] {
                    let active = self.persona == mode;
                    let response = mode_card(ui, card_width, mode, platform, title, subtitle, accent, active, self.connected);
                    if response.clicked() { self.switch_persona(mode); }
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("POWER & SLEEP").color(Color32::from_rgb(115, 130, 255)).strong());
                ui.label(RichText::new(&self.command_text).color(if self.command_ok { Color32::LIGHT_GREEN } else { Color32::LIGHT_RED }));
                if ui.add_enabled(self.controller_connected, egui::Button::new("Turn Off Controller")).clicked() {
                    let result = self.command(build_sleep_controller_report); self.outcome(result, "Controller is going to sleep");
                }
            });
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| { let mut v = self.last_status.as_ref().is_some_and(|s| s.idle_disconnect_enabled); if ui.add_enabled(self.connected, egui::Checkbox::new(&mut v, "Idle disconnect")).changed() { self.set_power("idle_disconnect", v); }
                    let mut v = self.last_status.as_ref().is_some_and(|s| s.usb_suspend_disconnect_enabled); if ui.add_enabled(self.connected, egui::Checkbox::new(&mut v, "USB suspend disconnect")).changed() { self.set_power("usb_suspend_disconnect", v); } });
                ui.horizontal(|ui| { let mut v = self.last_status.as_ref().is_some_and(|s| s.wake_on_connect_enabled); if ui.add_enabled(self.connected, egui::Checkbox::new(&mut v, "Wake on connect")).changed() { self.set_power("wake_on_connect", v); }
                    let mut v = self.last_status.as_ref().is_some_and(|s| s.sleep_keybind_enabled); if ui.add_enabled(self.connected, egui::Checkbox::new(&mut v, "Sleep keybind (PS button)")).changed() { self.set_power("sleep_keybind", v); } });
                ui.horizontal(|ui| { let enabled = self.connected && self.packet_mode_supported;
                    if ui.add_enabled(enabled, egui::Button::new("Single (0x36)").selected(self.packet_mode == AudioPacketMode::Single)).clicked() { self.choose_packet_mode(AudioPacketMode::Single); }
                    if ui.add_enabled(enabled, egui::Button::new("Double (0x39)").selected(self.packet_mode == AudioPacketMode::Double)).clicked() { self.choose_packet_mode(AudioPacketMode::Double); }
                    ui.label(RichText::new(if self.packet_mode_supported { "Single uses more bandwidth" } else { "Requires firmware protocol 1.24+" }).small().weak()); });
                ui.horizontal(|ui| {
                    ui.label("Disconnect controller after");
                    let response = ui.add_enabled(self.connected, egui::TextEdit::singleline(&mut self.idle_timeout_input).desired_width(48.0));
                    if response.changed() { self.idle_timeout_dirty = true; }
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let value = self.idle_timeout_input.clone();
                        self.submit_timeout(&value);
                    }
                    ui.label("minutes of inactivity");
                });
            });
            ui.add_space(7.0);
            ui.horizontal(|ui| { ui.label(RichText::new("GAME AUTO-SWITCHING").color(Color32::from_rgb(115, 130, 255)).strong()); ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.checkbox(&mut self.auto_switch_enabled, "Enabled"); }); });
            if self.config.auto_switch_enabled != self.auto_switch_enabled { self.config.auto_switch_enabled = self.auto_switch_enabled; self.config.save(); }
            egui::Frame::group(ui.style()).show(ui, |ui| { ui.horizontal(|ui| { ui.label(format!("Foreground process: {}", self.detected_game)); ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.label(&self.rule_status); }); }); });
            ui.horizontal(|ui| {
                let input_width = (ui.available_width() - 260.0).max(180.0);
                ui.add(egui::TextEdit::singleline(&mut self.new_process).hint_text("Process name (e.g. game.exe)").desired_width(input_width));
                for (name, mode) in [("Xbox", HostPersonaMode::Xbox), ("DualSense", HostPersonaMode::DualSense), ("Edge", HostPersonaMode::DualSenseEdge), ("DS4", HostPersonaMode::Ds4)] { if ui.button(name).clicked() { self.add_rule(mode); } }
            });
            let rules_height = (ui.available_height() - 48.0).max(110.0);
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), rules_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_min_height(rules_height - 14.0);
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                            if self.config.game_rules.is_empty() {
                                ui.centered_and_justified(|ui| {
                                    ui.label(RichText::new("No game rules yet. Add a game executable above to set its controller mode automatically.").weak());
                                });
                            }
                            let mut remove = None;
                            for rule in &self.config.game_rules {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("●").color(Color32::from_rgb(129, 140, 248)));
                                    ui.label(&rule.process_name);
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.small_button("Remove").clicked() { remove = Some(rule.id.clone()); }
                                        ui.label(RichText::new(rule.mode.display_name()).weak());
                                    });
                                });
                                ui.separator();
                            }
                            if let Some(id) = remove {
                                self.config.game_rules.retain(|r| r.id != id);
                                self.config.save();
                            }
                        });
                    });
                },
            );
            ui.horizontal(|ui| {
                if ui.button("Minimize to System Tray").clicked() { self.window_visible.store(false, Ordering::Relaxed); ui.ctx().send_viewport_cmd(egui::ViewportCommand::Visible(false)); }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("DS5 Bridge Companion · Rust 0.3.0").small().weak());
                });
            });
        });
    }
}

impl CompanionApp {
    fn choose_packet_mode(&mut self, mode: AudioPacketMode) {
        let value = mode.to_protocol_value();
        let result =
            self.command(|seq| build_value_report(COMMAND_ID_SET_AUDIO_PACKET_MODE, seq, value));
        if self.outcome(
            result,
            format!("Audio packet mode set to {}", mode.as_str()),
        ) {
            self.packet_mode = mode;
            self.config.audio_packet_mode = mode;
            self.config.save();
        }
    }
    fn submit_timeout(&mut self, text: &str) {
        if let Ok(value) = text.trim().parse::<u16>() {
            let value = clamp_idle_disconnect_timeout(value);
            let result = self.command(|seq| build_idle_disconnect_timeout_report(seq, value));
            if self.outcome(result, format!("Idle disconnect set to {value} min")) {
                self.idle_timeout = value;
                self.idle_timeout_input = value.to_string();
                self.idle_timeout_dirty = false;
            } else {
                self.idle_timeout_dirty = false;
                if let Some(status) = &self.last_status {
                    self.idle_timeout = status.idle_disconnect_timeout_minutes;
                    self.idle_timeout_input = self.idle_timeout.to_string();
                }
            }
        } else {
            self.command_text = "Enter a timeout between 1 and 120 minutes".into();
            self.command_ok = false;
            self.idle_timeout_dirty = false;
            if let Some(status) = &self.last_status {
                self.idle_timeout = status.idle_disconnect_timeout_minutes;
                self.idle_timeout_input = self.idle_timeout.to_string();
            }
        }
    }
    fn add_rule(&mut self, mode: HostPersonaMode) {
        let name = self.new_process.trim();
        if name.is_empty() {
            return;
        }
        let id = format!("rule-{}", self.config.game_rules.len() + 1);
        self.config.game_rules.push(config::GameRule {
            id,
            process_name: name.into(),
            mode,
        });
        self.new_process.clear();
        self.config.save();
    }
}

fn mode_card(
    ui: &mut egui::Ui,
    width: f32,
    mode: HostPersonaMode,
    platform: &str,
    title: &str,
    subtitle: &str,
    accent: Color32,
    active: bool,
    enabled: bool,
) -> egui::Response {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 112.0), sense);
    let painter = ui.painter();
    let fill = if active {
        Color32::from_rgb(37, 42, 63)
    } else if response.hovered() && enabled {
        Color32::from_rgb(34, 38, 48)
    } else {
        Color32::from_rgb(25, 28, 35)
    };
    let stroke = if active {
        egui::Stroke::new(1.5_f32, accent)
    } else {
        egui::Stroke::new(1.0_f32, Color32::from_rgb(46, 50, 61))
    };
    painter.rect_filled(rect, egui::CornerRadius::same(8), fill);
    painter.rect_stroke(rect, egui::CornerRadius::same(8), stroke, egui::StrokeKind::Inside);

    // Compact controller silhouette, with the Xbox face-button colors for
    // XInput mode and edge paddles for the DualSense Edge.
    let center = egui::pos2(rect.center().x, rect.top() + 25.0);
    let body = [
        egui::pos2(center.x - 24.0, center.y - 3.0),
        egui::pos2(center.x - 21.0, center.y - 11.0),
        egui::pos2(center.x - 15.0, center.y - 15.0),
        egui::pos2(center.x - 7.0, center.y - 14.0),
        egui::pos2(center.x - 3.0, center.y - 10.0),
        egui::pos2(center.x + 3.0, center.y - 10.0),
        egui::pos2(center.x + 7.0, center.y - 14.0),
        egui::pos2(center.x + 15.0, center.y - 15.0),
        egui::pos2(center.x + 21.0, center.y - 11.0),
        egui::pos2(center.x + 24.0, center.y - 3.0),
        egui::pos2(center.x + 23.0, center.y + 6.0),
        egui::pos2(center.x + 19.0, center.y + 9.0),
        egui::pos2(center.x + 15.0, center.y + 7.0),
        egui::pos2(center.x + 10.0, center.y + 1.0),
        egui::pos2(center.x - 10.0, center.y + 1.0),
        egui::pos2(center.x - 15.0, center.y + 7.0),
        egui::pos2(center.x - 19.0, center.y + 9.0),
        egui::pos2(center.x - 23.0, center.y + 6.0),
    ];
    let body_color = if enabled {
        Color32::from_rgb(211, 217, 229)
    } else {
        Color32::from_rgb(112, 116, 126)
    };
    painter.add(egui::Shape::convex_polygon(
        body.to_vec(),
        body_color,
        egui::Stroke::new(1.0_f32, Color32::from_rgb(236, 239, 245)),
    ));
    let detail = Color32::from_rgb(54, 60, 73);
    painter.rect_filled(
        egui::Rect::from_center_size(
            egui::pos2(center.x - 14.0, center.y - 2.0),
            Vec2::new(3.0, 11.0),
        ),
        1.0,
        detail,
    );
    painter.rect_filled(
        egui::Rect::from_center_size(
            egui::pos2(center.x - 14.0, center.y - 2.0),
            Vec2::new(11.0, 3.0),
        ),
        1.0,
        detail,
    );
    painter.rect_filled(
        egui::Rect::from_center_size(egui::pos2(center.x, center.y - 9.0), Vec2::new(10.0, 3.0)),
        1.0,
        Color32::from_rgb(154, 164, 183),
    );
    painter.circle_filled(egui::pos2(center.x - 6.0, center.y + 2.0), 2.4, detail);
    painter.circle_filled(egui::pos2(center.x + 5.0, center.y + 2.0), 2.4, detail);
    let face_colors = if mode == HostPersonaMode::Xbox {
        [
            Color32::from_rgb(74, 222, 128),
            Color32::from_rgb(248, 113, 113),
            Color32::from_rgb(96, 165, 250),
            Color32::from_rgb(250, 204, 21),
        ]
    } else {
        [accent, accent, accent, accent]
    };
    for (offset, color) in [
        (-4.0, face_colors[0]),
        (0.0, face_colors[1]),
        (4.0, face_colors[2]),
        (8.0, face_colors[3]),
    ] {
        painter.circle_filled(
            egui::pos2(center.x + 15.0 + offset * 0.35, center.y + offset),
            1.6,
            color,
        );
    }
    if mode == HostPersonaMode::DualSenseEdge {
        painter.line_segment(
            [
                egui::pos2(center.x - 16.0, center.y + 9.0),
                egui::pos2(center.x - 13.0, center.y + 12.0),
            ],
            egui::Stroke::new(2.0_f32, accent),
        );
        painter.line_segment(
            [
                egui::pos2(center.x + 16.0, center.y + 9.0),
                egui::pos2(center.x + 13.0, center.y + 12.0),
            ],
            egui::Stroke::new(2.0_f32, accent),
        );
    }

    let text_color = if enabled {
        Color32::from_rgb(228, 231, 238)
    } else {
        Color32::GRAY
    };
    painter.text(
        egui::pos2(rect.center().x, rect.top() + 57.0),
        egui::Align2::CENTER_CENTER,
        platform,
        egui::FontId::proportional(9.5),
        Color32::from_rgb(150, 157, 171),
    );
    painter.text(
        egui::pos2(rect.center().x, rect.top() + 75.0),
        egui::Align2::CENTER_CENTER,
        title,
        egui::FontId::proportional(13.0),
        if active { accent } else { text_color },
    );
    painter.text(
        egui::pos2(rect.center().x, rect.top() + 96.0),
        egui::Align2::CENTER_CENTER,
        subtitle,
        egui::FontId::proportional(9.5),
        Color32::from_rgb(150, 157, 171),
    );
    response
}

fn power_command(name: &str) -> u8 {
    use ds5_transport::protocol::{
        COMMAND_ID_SET_IDLE_DISCONNECT_ENABLED, COMMAND_ID_SET_SLEEP_KEYBIND_ENABLED,
        COMMAND_ID_SET_USB_SUSPEND_DISCONNECT_ENABLED, COMMAND_ID_SET_WAKE_ON_CONNECT,
    };
    match name {
        "idle_disconnect" => COMMAND_ID_SET_IDLE_DISCONNECT_ENABLED,
        "usb_suspend_disconnect" => COMMAND_ID_SET_USB_SUSPEND_DISCONNECT_ENABLED,
        "wake_on_connect" => COMMAND_ID_SET_WAKE_ON_CONNECT,
        "sleep_keybind" => COMMAND_ID_SET_SLEEP_KEYBIND_ENABLED,
        _ => unreachable!(),
    }
}
fn power_label(name: &str) -> &'static str {
    match name {
        "idle_disconnect" => "Idle disconnect",
        "usb_suspend_disconnect" => "USB suspend disconnect",
        "wake_on_connect" => "Wake on connect",
        "sleep_keybind" => "Sleep keybind",
        _ => "Setting",
    }
}

fn main() -> eframe::Result<()> {
    let mut options = eframe::NativeOptions::default();
    options.viewport = egui::ViewportBuilder::default()
        .with_inner_size([640.0, 860.0])
        .with_min_inner_size([580.0, 650.0])
        .with_icon(load_window_icon());
    eframe::run_native(
        "DS5 Bridge Companion",
        options,
        Box::new(|cc| {
            let mut visuals = egui::Visuals::dark();
            visuals.panel_fill = Color32::from_rgb(18, 20, 26);
            visuals.window_fill = Color32::from_rgb(23, 25, 32);
            visuals.extreme_bg_color = Color32::from_rgb(14, 16, 21);
            visuals.faint_bg_color = Color32::from_rgb(27, 30, 38);
            visuals.selection.bg_fill = Color32::from_rgb(50, 64, 112);
            visuals.selection.stroke = egui::Stroke::new(1.0_f32, Color32::from_rgb(111, 127, 255));
            visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(27, 30, 38);
            visuals.widgets.noninteractive.bg_stroke =
                egui::Stroke::new(1.0_f32, Color32::from_rgb(43, 47, 58));
            visuals.widgets.inactive.bg_fill = Color32::from_rgb(34, 37, 45);
            visuals.widgets.inactive.bg_stroke =
                egui::Stroke::new(1.0_f32, Color32::from_rgb(48, 52, 64));
            visuals.widgets.hovered.bg_fill = Color32::from_rgb(48, 53, 67);
            visuals.widgets.hovered.bg_stroke =
                egui::Stroke::new(1.0_f32, Color32::from_rgb(91, 104, 151));
            visuals.widgets.active.bg_fill = Color32::from_rgb(50, 64, 112);
            visuals.widgets.active.bg_stroke =
                egui::Stroke::new(1.0_f32, Color32::from_rgb(111, 127, 255));
            cc.egui_ctx.set_visuals(visuals);
            cc.egui_ctx.style_mut_of(egui::Theme::Dark, |style| {
                style.spacing.item_spacing = Vec2::new(9.0, 8.0);
                style.spacing.button_padding = Vec2::new(11.0, 7.0);
                style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(8);
                style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(7);
                style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(7);
                style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(7);
            });
            Ok(Box::new(CompanionApp::new(&cc.egui_ctx)))
        }),
    )
}

fn load_window_icon() -> Arc<egui::IconData> {
    let bytes = include_bytes!("../assets/icon.png");
    let image = image::load_from_memory(bytes)
        .expect("embedded app icon must be a valid PNG")
        .into_rgba8();
    Arc::new(egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}
