use pv_recorder::RecorderBuilder;

#[tauri::command]
pub fn pv_get_audio_devices() -> Vec<String> {
    let audio_devices = RecorderBuilder::default().get_audio_devices();
    match audio_devices {
        Ok(audio_devices) => audio_devices,
        Err(err) => panic!("Failed to get audio devices: {}", err),
    }
}

#[tauri::command]
pub fn pv_get_audio_device_name(idx: i32) -> String {
    let audio_devices = RecorderBuilder::default().get_audio_devices();
    let mut first_device: String = String::new();
    match audio_devices {
        Ok(audio_devices) => {
            for (_idx, device) in audio_devices.iter().enumerate() {
                if idx as usize == _idx {
                    return device.to_string();
                }

                if _idx == 0 {
                    first_device = device.to_string()
                }
            }
        }
        Err(err) => panic!("Failed to get audio devices: {}", err),
    };

    // return first device as default, if none were matched
    first_device
}
