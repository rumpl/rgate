mod app;
mod assets;
#[cfg(target_family = "wasm")]
mod browser;
mod canvas;
mod classic_gates;
mod commands;
mod components;
mod dialogs;
mod input;
mod label_layout;
mod led;
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
    application
        .with_assets(assets::Assets)
        .run(move |cx: &mut App| {
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
            gpui_component::init(cx);
            commands::install(cx);
            let bounds = Bounds::centered(None, size(px(1240.0), px(820.0)), cx);
            // GPUI Kit's Root hosts the window overlays: dialogs, popups and tooltips.
            let opened = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(1020.0), px(620.0))),
                    ..Default::default()
                },
                move |window, cx| {
                    let app = cx.new(|cx| {
                        let mut app = app::GateApp::new(circuit, path, window, cx);
                        app.enable_persistence(cx);
                        for warning in warnings {
                            app.log(warning);
                        }
                        app
                    });
                    cx.new(|cx| gpui_component::Root::new(app, window, cx))
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

mod live_simulation;

mod verilog_editor;

#[cfg(not(target_family = "wasm"))]
mod source_editor;

mod hdl_diagnostics;

#[cfg(feature = "icarus")]
mod compilation;
