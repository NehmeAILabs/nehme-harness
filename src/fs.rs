pub fn expand_tilde(s: &str) -> String {
    let home = || dirs::home_dir().map(|p| p.to_string_lossy().to_string());

    if s == "~" || s == "$HOME" {
        if let Some(h) = home() {
            return h;
        }
        return s.to_string();
    }
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(h) = home() {
            return std::path::Path::new(&h)
                .join(rest)
                .to_string_lossy()
                .to_string();
        }
        return s.to_string();
    }
    if let Some(rest) = s.strip_prefix("$HOME/")
        && let Some(h) = home()
    {
        return std::path::Path::new(&h)
            .join(rest)
            .to_string_lossy()
            .to_string();
    }
    s.to_string()
}
