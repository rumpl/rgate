mod app;
#[cfg(target_family = "wasm")]
mod browser;
mod canvas;
mod classic_gates;
mod classic_icons;
mod commands;
mod components;
mod icons;
mod input;
mod label_layout;
mod led;
mod message_log;
mod modern_gates;
mod properties;
mod scope;
mod symbol_editor;
mod theme;
mod vector;
mod vga_display;
mod view;
mod waveform;
mod waveform_view;
mod workspace;

use anyhow::Result;
use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use rgate_core::Circuit;
use std::path::PathBuf;

pub fn run(circuit: Circuit, path: Option<PathBuf>, warnings: Vec<String>) -> Result<()> {
    circuit.validate()?;
    #[cfg(not(target_family = "wasm"))]
    let application = gpui_platform::application();
    #[cfg(target_family = "wasm")]
    let application = {
        let platform = std::rc::Rc::new(gpui_web::WebPlatform::new(false));
        let http = std::sync::Arc::new(platform.fetch_http_client());
        gpui::Application::with_platform(platform).with_http_client(http)
    };
    application.run(move |cx: &mut App| {
        #[cfg(target_family = "wasm")]
        if let Err(error) = cx.text_system().add_fonts(vec![
            std::borrow::Cow::Borrowed(
                include_bytes!("../../../assets/fonts/IBMPlexSans-Regular.ttf").as_slice(),
            ),
            std::borrow::Cow::Borrowed(
                include_bytes!("../../../assets/fonts/IBMPlexSans-SemiBold.ttf").as_slice(),
            ),
            std::borrow::Cow::Borrowed(
                include_bytes!("../../../assets/fonts/Lilex-Regular.ttf").as_slice(),
            ),
            std::borrow::Cow::Borrowed(
                include_bytes!("../../../assets/fonts/Lilex-Bold.ttf").as_slice(),
            ),
            std::borrow::Cow::Borrowed(
                include_bytes!("../../../assets/fonts/NotoSansSymbols2-Regular.ttf").as_slice(),
            ),
            std::borrow::Cow::Borrowed(
                include_bytes!("../../../assets/fonts/NotoSansMath-Regular.ttf").as_slice(),
            ),
        ]) {
            eprintln!("Browser fonts could not load: {error:#}");
            cx.quit();
            return;
        }
        commands::install(cx);
        let bounds = Bounds::centered(None, size(px(1240.0), px(820.0)), cx);
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(1020.0), px(620.0))),
                ..Default::default()
            },
            move |window, cx| {
                cx.new(|cx| {
                    let mut app = app::GateApp::new(circuit, path, window, cx);
                    app.enable_persistence(cx);
                    for warning in warnings {
                        app.log(warning);
                    }
                    app
                })
            },
        );
        if let Err(error) = opened {
            eprintln!("Could not open RGate window: {error:#}");
            cx.quit();
            return;
        }
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        cx.activate(true);
    });
    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod theme_tests;

#[cfg(test)]
mod menu_tests;

#[cfg(test)]
mod component_tests;

#[cfg(test)]
mod property_tests;

#[cfg(test)]
mod dialog_tests;

mod live_simulation;

mod verilog_editor;

#[cfg(test)]
mod viewport_tests;

#[cfg(not(target_family = "wasm"))]
mod source_editor;
