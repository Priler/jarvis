mod menu;

use tray_icon::{
    menu::{AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayEvent, TrayIconBuilder,
};
use winit::event_loop::{ControlFlow, EventLoopBuilder};
use image;
use winit::platform::windows::EventLoopBuilderExtWindows;

use crate::config;

pub fn init() {
    // spawn tray icon
    // New thread will prevent tray icon to work on MacOS
    // @TODO: MacOS support.
    std::thread::spawn(|| {

        // load tray icon
        let icon_path = format!("{}/icons/{}", env!("CARGO_MANIFEST_DIR"), config::TRAY_ICON);
        let icon = load_icon(std::path::Path::new(&icon_path));

        // form tray menu
        let tray_menu = Menu::with_items(&[
            &MenuItem::new("Перезапуск", true, None),
            &MenuItem::new("Настройки", true, None),
            &MenuItem::new("Выход", true, None)
        ]);

        #[cfg(not(target_os = "linux"))]
        let mut tray_icon = Some(
            TrayIconBuilder::new()
                .with_menu(Box::new(tray_menu))
                .with_tooltip(config::TRAY_TOOLTIP)
                .with_icon(icon)
                .build()
                .unwrap()
        );

        // Since winit doesn't use gtk on Linux, and we need gtk for
        // the tray icon to show up, we need to initialize gtk and create the tray_icon
        #[cfg(target_os = "linux")]
        {
            use tray_icon::menu::Menu;

            gtk::init().unwrap();
            let _tray_icon = TrayIconBuilder::new()
                .with_menu(Box::new(tray_menu))
                .with_tooltip(config::TRAY_TOOLTIP)
                .with_icon(icon)
                .build()
                .unwrap();

            gtk::main();
        }

        // run the event loop
        let event_loop = EventLoopBuilder::new().with_any_thread(true).build();

        let menu_channel = MenuEvent::receiver();
        let tray_channel = TrayEvent::receiver();

        event_loop.run(move |_event, _, control_flow| {
            *control_flow = ControlFlow::Poll;

            //if let Ok(event) = tray_channel.try_recv() {
            //    println!("tray event: {event:?}");
            //}

            if let Ok(event) = menu_channel.try_recv() {
                println!("menu event: {:?}", event);

                if event.id == 1002 {
                    std::process::exit(0);
                }
            }
        });

    });

    info!("Tray initialized.");
}

fn load_icon(path: &std::path::Path) -> tray_icon::icon::Icon {
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::open(path)
            .expect("Failed to open icon path")
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    tray_icon::icon::Icon::from_rgba(icon_rgba, icon_width, icon_height)
        .expect("Failed to open icon")
}