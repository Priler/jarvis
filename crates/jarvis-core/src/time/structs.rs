use chrono::Timelike;

#[derive(Debug, Clone, Copy)]
pub enum TimeOfDay {
    Morning, // 5:00 - 11:59
    Day,     // 12:00 - 16:59
    Evening, // 17:00 - 21:59
    Night,   // 22:00 - 4:59
}

impl TimeOfDay {
    pub fn now() -> Self {
        let hour = chrono::Local::now().hour();
        match hour {
            5..=11 => TimeOfDay::Morning,
            12..=16 => TimeOfDay::Day,
            17..=21 => TimeOfDay::Evening,
            _ => TimeOfDay::Night,
        }
    }
}
