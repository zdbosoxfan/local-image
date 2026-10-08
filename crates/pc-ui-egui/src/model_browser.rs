//! local-image: the Model Browser window (models and LoRAs from Hugging Face, Civitai, ComfyUI's
//! templates and the community list).

use std::cell::RefCell;

thread_local! { static OPEN: RefCell<bool> = const { RefCell::new(false) }; }

/// Opens the browser window.
pub fn open() {
    OPEN.with(|o| *o.borrow_mut() = true);
}

pub fn is_open() -> bool {
    OPEN.with(|o| *o.borrow())
}
