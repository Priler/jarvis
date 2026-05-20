#![allow(dead_code)]

pub mod screen;
pub use screen::ScreenContext;

use crate::bus::{BusEvent, CognitiveBus};
use std::sync::Arc;

pub struct PerceptionLayer {
    last_screen: Option<ScreenContext>,
    last_screen_title: String,
    bus: Arc<CognitiveBus>,
}

impl PerceptionLayer {
    pub fn new(bus: Arc<CognitiveBus>) -> Self {
        Self {
            last_screen: None,
            last_screen_title: String::new(),
            bus,
        }
    }

    /// Poll the screen state. Publishes `BusEvent::ScreenChanged` when the active window changes.
    pub fn poll_screen(&mut self) -> Option<&ScreenContext> {
        let ctx = screen::get_active_window()?;

        if ctx.window_title != self.last_screen_title {
            self.last_screen_title = ctx.window_title.clone();
            self.bus.publish(BusEvent::ScreenChanged {
                window_title: ctx.window_title.clone(),
                is_browser: ctx.is_browser,
                is_media: ctx.is_media,
            });
            debug!("[PERCEPTION] Active window: '{}'", ctx.window_title);
        }

        self.last_screen = Some(ctx);
        self.last_screen.as_ref()
    }

    pub fn last_screen_context(&self) -> Option<&ScreenContext> {
        self.last_screen.as_ref()
    }

    /// Returns a domain hint if the current screen context suggests a specific domain.
    pub fn screen_domain_hint(&self) -> Option<&'static str> {
        self.last_screen.as_ref().and_then(|ctx| {
            if ctx.is_browser { Some("browser") }
            else if ctx.is_media { Some("media") }
            else { None }
        })
    }
}
