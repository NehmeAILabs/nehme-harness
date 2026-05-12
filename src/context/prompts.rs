use std::collections::HashMap;
use std::path::PathBuf;

use include_dir::{Dir, include_dir};

static EMBEDDED: Dir = include_dir!("$CARGO_MANIFEST_DIR/prompts");

pub fn global_dir() -> PathBuf {
    crate::session::storage::data_dir().join("prompts")
}

pub fn load() -> HashMap<String, String> {
    let mut prompts: HashMap<String, String> = HashMap::new();

    for (name, content) in crate::context::load_embedded_files(&EMBEDDED, "md") {
        prompts.entry(name).or_insert(content);
    }
    for (name, content) in crate::context::load_dir_files(&global_dir(), "md") {
        prompts.insert(name, content);
    }
    for (name, content) in crate::context::load_dir_files(&PathBuf::from("prompts"), "md") {
        prompts.insert(name, content);
    }

    prompts
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
