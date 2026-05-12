use std::collections::HashMap;
use std::path::{Path, PathBuf};

use include_dir::Dir;

pub fn load_files(embedded: &Dir, global_dir: &Path, local_dir: &Path, ext: &str) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();

    for file in embedded.files() {
        if file.path().extension().is_some_and(|e| e == ext)
            && let Some(name) = file.path().file_stem().and_then(|s| s.to_str())
            && let Some(content) = file.contents_utf8()
        {
            map.entry(name.to_string())
                .or_insert_with(|| content.to_string());
        }
    }

    for dir in [global_dir, local_dir] {
        if dir.exists()
            && let Ok(entries) = std::fs::read_dir(dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == ext)
                    && let Some(name) = path.file_stem().and_then(|s| s.to_str())
                    && let Ok(content) = std::fs::read_to_string(&path)
                {
                    map.insert(name.to_string(), content);
                }
            }
        }
    }

    map
}

pub fn ensure_global(embedded: &Dir, dir: &Path) -> anyhow::Result<()> {
    if !dir.exists() {
        std::fs::create_dir_all(dir)?;
        copy_embedded(embedded, dir)?;
    }
    Ok(())
}

pub fn regen(embedded: &Dir, dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    copy_embedded(embedded, dir)
}

fn copy_embedded(embedded: &Dir, dest: &Path) -> anyhow::Result<()> {
    for file in embedded.files() {
        if let Some(name) = file.path().file_name().and_then(|s| s.to_str()) {
            let dest_path = dest.join(name);
            if let Some(content) = file.contents_utf8() {
                std::fs::write(&dest_path, content)?;
            }
        }
    }
    Ok(())
}
