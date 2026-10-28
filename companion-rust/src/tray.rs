use std::sync::mpsc::Sender;
use std::sync::Arc;

use image::GenericImageView;
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub struct TrayManager {
    _tray_icon: TrayIcon,
    _menu: Menu,
}

impl TrayManager {
    pub fn new(
        sender: Sender<TrayAction>,
        request_repaint: eframe::egui::Context,
        toggle_window: Arc<dyn Fn() + Send + Sync>,
        request_exit: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let menu = Menu::new();
        let item_dualsense = MenuItem::new("DualSense (PS5)", true, None);
        let item_edge = MenuItem::new("DualSense Edge", true, None);
        let item_ds4 = MenuItem::new("DualShock 4 (PS4)", true, None);
        let item_xbox = MenuItem::new("Xbox 360 (XInput)", true, None);
        let item_show = MenuItem::new("Show / Hide Window", true, None);
        let item_exit = MenuItem::new("Exit DS5 Bridge Companion", true, None);

        let id_dualsense = item_dualsense.id().0.clone();
        let id_edge = item_edge.id().0.clone();
        let id_ds4 = item_ds4.id().0.clone();
        let id_xbox = item_xbox.id().0.clone();
        let id_show = item_show.id().0.clone();
        let id_exit = item_exit.id().0.clone();

        menu.append(&item_dualsense)?;
        menu.append(&item_edge)?;
        menu.append(&item_ds4)?;
        menu.append(&item_xbox)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&item_show)?;
        menu.append(&item_exit)?;

        let tray_icon = TrayIconBuilder::new()
            .with_tooltip("DS5 Bridge Companion")
            .with_icon(load_icon()?)
            .with_menu(Box::new(menu.clone()))
            .with_menu_on_left_click(false)
            .build()?;

        // Native tray callbacks run independently of egui's window updates.
        // This keeps tray actions alive while the main window is hidden.
        let menu_sender = sender.clone();
        let menu_ctx = request_repaint.clone();
        let show_callback = toggle_window.clone();
        let exit_callback = request_exit.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let id = event.id().0.as_str();
            let action = if id == id_dualsense {
                Some(TrayAction::SetPersona("dualsense"))
            } else if id == id_edge {
                Some(TrayAction::SetPersona("dualsense-edge"))
            } else if id == id_ds4 {
                Some(TrayAction::SetPersona("ds4"))
            } else if id == id_xbox {
                Some(TrayAction::SetPersona("xbox"))
            } else if id == id_show {
                show_callback();
                None
            } else if id == id_exit {
                exit_callback();
                None
            } else {
                None
            };
            if let Some(action) = action {
                let _ = menu_sender.send(action);
            }
            menu_ctx.request_repaint();
        }));

        let icon_ctx = request_repaint;
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
                | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } => toggle_window(),
                _ => {}
            }
            icon_ctx.request_repaint();
        }));

        Ok(Self {
            _tray_icon: tray_icon,
            _menu: menu,
        })
    }
}

impl Drop for TrayManager {
    fn drop(&mut self) {
        MenuEvent::set_event_handler(None::<fn(MenuEvent)>);
        TrayIconEvent::set_event_handler(None::<fn(TrayIconEvent)>);
    }
}

pub enum TrayAction {
    SetPersona(&'static str),
}

fn load_icon() -> Result<tray_icon::Icon, Box<dyn std::error::Error>> {
    let bytes = include_bytes!("../assets/icon.png");
    let image = image::load_from_memory(bytes)?;
    let (width, height) = image.dimensions();
    let rgba = image.into_rgba8().into_raw();
    Ok(tray_icon::Icon::from_rgba(rgba, width, height)?)
}
