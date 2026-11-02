mod menu;

use image;
use std::process::Command;
use tray_icon::{menu::MenuEvent, TrayIconBuilder};

#[cfg(target_os = "windows")]
use winit::platform::windows::EventLoopBuilderExtWindows;

use jarvis_core::{
    config, i18n,
    ipc::{self, IpcEvent},
    voices, SettingsManager,
};

const TRAY_ICON_BYTES: &[u8] = include_bytes!("../../../resources/icons/32x32.png");

pub fn init_blocking(settings: SettingsManager) {
    let icon = load_icon_from_bytes(TRAY_ICON_BYTES);

    // build menu with settings submenus
    let tray_menu = menu::build(&settings);
    let menu::TrayMenu {
        menu,
        state: tray_state,
    } = tray_menu;

    let _tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(i18n::t("tray-tooltip"))
        .with_icon(icon)
        .build()
        .unwrap();

    let menu_channel = MenuEvent::receiver();

    #[cfg(target_os = "linux")]
    {
        gtk::init().unwrap();
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            if let Ok(event) = menu_channel.try_recv() {
                handle_menu_event(&event, &settings, &tray_state);
            }
            glib::ControlFlow::Continue
        });
        gtk::main();
    }

    #[cfg(target_os = "macos")]
    {
        use winit::event_loop::{ControlFlow, EventLoop};
        let event_loop = EventLoop::new().unwrap();
        event_loop
            .run(move |_event, elwt| {
                elwt.set_control_flow(ControlFlow::Wait);
                if let Ok(event) = menu_channel.try_recv() {
                    handle_menu_event(&event, &settings, &tray_state);
                }
            })
            .unwrap();
    }

    #[cfg(target_os = "windows")]
    {
        loop {
            if let Ok(event) = menu_channel.try_recv() {
                handle_menu_event(&event, &settings, &tray_state);
            }

            // pump Windows messages
            unsafe {
                let mut msg: winapi::um::winuser::MSG = std::mem::zeroed();
                while winapi::um::winuser::PeekMessageW(
                    &mut msg,
                    std::ptr::null_mut(),
                    0,
                    0,
                    winapi::um::winuser::PM_REMOVE,
                ) != 0
                {
                    winapi::um::winuser::TranslateMessage(&msg);
                    winapi::um::winuser::DispatchMessageW(&msg);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    info!("Tray initialized.");
}

fn handle_menu_event(event: &MenuEvent, settings: &SettingsManager, tray_state: &menu::TrayState) {
    let id = event.id.0.as_str();

    // -- radio group: "set:key:value"
    if let Some(rest) = id.strip_prefix("set:") {
        if let Some((key, value)) = rest.split_once(':') {
            match settings.write(key, value) {
                Ok(()) => {
                    info!("Tray: {} = {}", key, value);

                    // update check marks in the radio group
                    for group in &tray_state.radio_groups {
                        if group.setting_key == key {
                            group.select(value);
                            break;
                        }
                    }

                    // apply side effects
                    match key {
                        "language" => {
                            i18n::set_language(value);
                        }
                        "assistant_voice" => {
                            voices::set_current_voice(value);
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    warn!("Tray: failed to set {} = {}: {}", key, value, e);
                }
            }
            return;
        }
    }

    // -- toggle: "toggle:key"
    if let Some(key) = id.strip_prefix("toggle:") {
        match key {
            "gain_normalizer" => {
                // CheckMenuItem auto-toggles on click, just read the new state
                let new_val = tray_state.gain_toggle.is_checked();
                let val_str = if new_val { "true" } else { "false" };
                if let Err(e) = settings.write(key, val_str) {
                    warn!("Tray: failed to toggle {}: {}", key, e);
                    // revert visual state on error
                    tray_state.gain_toggle.set_checked(!new_val);
                } else {
                    info!("Tray: {} = {}", key, val_str);
                }
            }
            _ => {}
        }
        return;
    }

    // -- action items
    match id {
        "exit" => std::process::exit(0),
        "restart" => {
            info!("Restarting from tray menu...");
            restart_app();
        }
        "settings" => {
            info!("Opening settings from tray menu...");
            open_settings();
        }
        _ => {}
    }
}

// HELPERS

fn load_icon_from_bytes(bytes: &[u8]) -> tray_icon::Icon {
    let image = image::load_from_memory(bytes)
        .expect("Failed to load icon")
        .into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    tray_icon::Icon::from_rgba(rgba, width, height).expect("Failed to create icon")
}

fn restart_app() {
    let exe_path = match std::env::current_exe() {
        Ok(path) => path,
        Err(e) => {
            error!("Failed to get executable path: {}", e);
            return;
        }
    };

    match Command::new(&exe_path).spawn() {
        Ok(_) => {
            info!("Spawned new instance, exiting current...");
            std::process::exit(0);
        }
        Err(e) => {
            error!("Failed to restart: {}", e);
        }
    }
}

fn open_settings() {
    if ipc::has_clients() {
        info!("GUI is connected, sending reveal event");
        ipc::send(IpcEvent::RevealWindow);
    } else {
        info!("GUI not connected, launching jarvis-gui");
        launch_gui();
    }
}

fn launch_gui() {
    let exe_path = match std::env::current_exe() {
        Ok(path) => path,
        Err(e) => {
            error!("Failed to get executable path: {}", e);
            return;
        }
    };

    let gui_path = exe_path
        .parent()
        .map(|p| p.join(get_gui_executable_name()))
        .unwrap_or_else(|| get_gui_executable_name().into());

    info!("Launching GUI: {:?}", gui_path);

    match Command::new(&gui_path).spawn() {
        Ok(_) => info!("Launched jarvis-gui"),
        Err(e) => error!("Failed to launch jarvis-gui: {}", e),
    }
}

#[cfg(target_os = "windows")]
fn get_gui_executable_name() -> &'static str {
    "jarvis-gui.exe"
}

#[cfg(not(target_os = "windows"))]
fn get_gui_executable_name() -> &'static str {
    "jarvis-gui"
}
