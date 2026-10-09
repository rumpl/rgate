use anyhow::{Result, anyhow};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
export function rgatePickFile(accept) {
    return new Promise((resolve, reject) => {
        const input = document.createElement('input');
        input.type = 'file'; input.accept = accept; input.hidden = true;
        document.body.appendChild(input);
        const finish = value => { input.remove(); resolve(value); };
        input.addEventListener('cancel', () => finish(null), {once: true});
        input.addEventListener('change', async () => {
            const file = input.files?.[0];
            if (!file) { finish(null); return; }
            try {
                const bytes = new Uint8Array(await file.arrayBuffer());
                finish({name: file.name, bytes});
            } catch (error) { input.remove(); reject(error); }
        }, {once: true});
        input.click();
    });
}
export function rgateDownload(name, content) {
    const blob = new Blob([content], {type: 'text/plain;charset=utf-8'});
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a'); link.href = url; link.download = name;
    document.body.appendChild(link); link.click(); link.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
}
export function rgateConfirmDiscard() { return window.confirm('Discard unsaved circuit changes? Save/download the circuit first to keep your edits.'); }
export function rgateStorageRead(name) { return localStorage.getItem(`rgate.v1.${name}`); }
export function rgateStorageWrite(name, content) { localStorage.setItem(`rgate.v1.${name}`, content); }
export function rgateStorageRemove(name) { localStorage.removeItem(`rgate.v1.${name}`); }
export function rgateSetDirty(dirty) {
    window.onbeforeunload = dirty ? event => { event.preventDefault(); event.returnValue = ''; } : null;
}
"#)]
extern "C" {
    #[wasm_bindgen(catch, js_name = rgateStorageRead)]
    pub fn storage_read(name: &str) -> Result<Option<String>, JsValue>;
    #[wasm_bindgen(catch, js_name = rgateStorageWrite)]
    pub fn storage_write(name: &str, content: &str) -> Result<(), JsValue>;
    #[wasm_bindgen(catch, js_name = rgateStorageRemove)]
    pub fn storage_remove(name: &str) -> Result<(), JsValue>;
    #[wasm_bindgen(js_name = rgatePickFile)]
    fn pick_file(accept: &str) -> js_sys::Promise;
    #[wasm_bindgen(catch, js_name = rgateDownload)]
    pub fn download(name: &str, content: &str) -> Result<(), JsValue>;
    #[wasm_bindgen(js_name = rgateConfirmDiscard)]
    pub fn confirm_discard() -> bool;
    #[wasm_bindgen(js_name = rgateSetDirty)]
    pub fn set_dirty(dirty: bool);
}

pub fn picker(accept: &str) -> js_sys::Promise {
    pick_file(accept)
}

pub async fn read_file(promise: js_sys::Promise) -> Result<Option<(String, String)>> {
    let value = wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(js_error)?;
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    let name = js_sys::Reflect::get(&value, &"name".into())
        .map_err(js_error)?
        .as_string()
        .ok_or_else(|| anyhow!("uploaded file has no name"))?;
    let bytes = js_sys::Reflect::get(&value, &"bytes".into()).map_err(js_error)?;
    let bytes = js_sys::Uint8Array::new(&bytes).to_vec();
    // Keep TkGate's ISO-8859-1 examples readable, as the native importer does.
    let content = String::from_utf8(bytes)
        .unwrap_or_else(|error| error.into_bytes().into_iter().map(char::from).collect());
    Ok(Some((name, content)))
}

pub fn js_error(value: JsValue) -> anyhow::Error {
    anyhow!(
        "browser operation failed: {}",
        value.as_string().unwrap_or_else(|| format!("{value:?}"))
    )
}
