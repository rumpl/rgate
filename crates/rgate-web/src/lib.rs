#[cfg(target_family = "wasm")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn start() -> Result<(), wasm_bindgen::JsValue> {
    console_error_panic_hook::set_once();
    gpui_web::init_logging();
    rgate_ui::run(rgate_core::demo::full_adder(), None, Vec::new())
        .map_err(|error| wasm_bindgen::JsValue::from_str(&format!("{error:#}")))
}
