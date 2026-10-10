use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState};
use muda::{Menu, MenuItem, PredefinedMenuItem, MenuEvent};
use image::GenericImageView;

pub struct TrayManager {
    _tray_icon: TrayIcon,
    _menu: Menu,
    pub id_dualsense: String,
    pub id_edge: String,
    pub id_ds4: String,
    pub id_xbox: String,
    pub id_show: String,
    pub id_exit: String,
}

impl TrayManager {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let menu = Menu::new();

        let item_dualsense = MenuItem::new("🎮 DualSense Mode (PS5)", true, None);
        let item_edge = MenuItem::new("🎮 DualSense Edge Mode (PS5)", true, None);
        let item_ds4 = MenuItem::new("🎮 DualShock 4 Mode (PS4)", true, None);
        let item_xbox = MenuItem::new("🎮 Xbox 360 Mode (XInput)", true, None);
        let item_show = MenuItem::new("🖥️ Show / Hide Window", true, None);
        let item_exit = MenuItem::new("❌ Exit Companion", true, None);

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

        let icon = load_icon()?;

        let tray_icon = TrayIconBuilder::new()
            .with_tooltip("DS5 Bridge Companion")
            .with_icon(icon)
            .with_menu(Box::new(menu.clone()))
            // Left click must toggle the window; only right click opens the menu.
            .with_menu_on_left_click(false)
            .build()?;

        Ok(Self {
            _tray_icon: tray_icon,
            _menu: menu,
            id_dualsense,
            id_edge,
            id_ds4,
            id_xbox,
            id_show,
            id_exit,
        })
    }

    pub fn poll_menu_events(&self) -> Option<TrayAction> {
        let mut action = None;

        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let id = event.id().0.as_str();
            if id == self.id_dualsense {
                action = Some(TrayAction::SetPersona("dualsense"));
            } else if id == self.id_edge {
                action = Some(TrayAction::SetPersona("dualsense-edge"));
            } else if id == self.id_ds4 {
                action = Some(TrayAction::SetPersona("ds4"));
            } else if id == self.id_xbox {
                action = Some(TrayAction::SetPersona("xbox"));
            } else if id == self.id_show {
                action = Some(TrayAction::ToggleWindow);
            } else if id == self.id_exit {
                action = Some(TrayAction::Exit);
            }
        }

        // A menu selection arriving in the same tick must not be discarded by a
        // stray icon click, so keep both and let the menu win.
        let mut icon_action = None;
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            match event {
                TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
                | TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => {
                    icon_action = Some(TrayAction::ToggleWindow);
                }
                _ => {}
            }
        }

        action.or(icon_action)
    }
}

pub enum TrayAction {
    SetPersona(&'static str),
    ToggleWindow,
    Exit,
}

fn load_icon() -> Result<tray_icon::Icon, Box<dyn std::error::Error>> {
    let bytes = include_bytes!("../assets/icon.png");
    let image = image::load_from_memory(bytes)?;
    let (width, height) = image.dimensions();
    let rgba = image.into_rgba8().into_raw();
    Ok(tray_icon::Icon::from_rgba(rgba, width, height)?)
}
