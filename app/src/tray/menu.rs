pub enum TrayMenuItem {
    Restart,
    Settings,
    Exit
}

impl TrayMenuItem {
    pub fn label(&self) -> &str {
        match *self {
            TrayMenuItem::Restart => "Перезапустить",
            TrayMenuItem::Settings => "Настройки",
            TrayMenuItem::Exit => "Выход"
        }
    }
}