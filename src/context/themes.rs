use std::collections::HashMap;
use std::path::PathBuf;

use include_dir::{Dir, include_dir};

use crate::config::ColorsConfig;
use crate::ui::renderer::Renderer;
use crate::ui::utils::parse_color;

static EMBEDDED: Dir = include_dir!("$CARGO_MANIFEST_DIR/themes");

pub fn global_dir() -> PathBuf {
    crate::session::storage::data_dir().join("themes")
}

pub fn load() -> HashMap<String, String> {
    let mut themes: HashMap<String, String> = HashMap::new();

    for (name, content) in crate::context::load_embedded_files(&EMBEDDED, "json") {
        themes.entry(name).or_insert(content);
    }
    for (name, content) in crate::context::load_dir_files(&global_dir(), "json") {
        themes.insert(name, content);
    }
    for (name, content) in crate::context::load_dir_files(&PathBuf::from("themes"), "json") {
        themes.insert(name, content);
    }

    themes
}

pub fn ensure_global() -> anyhow::Result<()> {
    let dir = global_dir();
    if !dir.exists() {
        crate::context::copy_embedded_to(&EMBEDDED, &dir)?;
    }
    Ok(())
}

pub fn regen() -> anyhow::Result<()> {
    let dir = global_dir();
    crate::context::copy_embedded_to(&EMBEDDED, &dir)
}

pub fn apply(content: &str, renderer: &mut Renderer) {
    if let Ok(colors) = serde_json::from_str::<ColorsConfig>(content) {
        let chat_bg = colors.chat_background.as_deref().and_then(parse_color);
        let input_bg = colors.input_background.as_deref().and_then(parse_color);
        let status_bg = colors.status_background.as_deref().and_then(parse_color);
        renderer.set_background_colors(chat_bg, input_bg, status_bg);
    }
}
