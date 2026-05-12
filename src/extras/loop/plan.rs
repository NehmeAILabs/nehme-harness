use std::io::Write;
use std::path::Path;

use super::DEFAULT_PLAN_FILENAME;

pub fn plan_exists(plan_file: &Path) -> bool {
    plan_file.exists()
}

pub fn read_plan(plan_file: &Path) -> Option<String> {
    if plan_file.exists() {
        std::fs::read_to_string(plan_file).ok()
    } else {
        None
    }
}

pub fn delete_plan(plan_file: &Path) {
    if plan_file.exists() {
        let _ = std::fs::remove_file(plan_file);
    }
}

pub async fn handle_startup(plan_file: &Path) -> anyhow::Result<bool> {
    if !plan_exists(plan_file) {
        return Ok(false);
    }
    eprint!(
        "{} already exists. Restart from existing plan? [Y/n] ",
        DEFAULT_PLAN_FILENAME
    );
    let _ = std::io::stdout().flush();
    let plan_file_owned = plan_file.to_path_buf();
    let input = tokio::task::spawn_blocking(move || {
        let mut input = String::new();
        let _ = std::io::stdin().read_line(&mut input);
        input
    })
    .await?;
    let input = input.trim().to_lowercase();
    if input == "n" || input == "no" {
        delete_plan(&plan_file_owned);
        Ok(false)
    } else {
        Ok(true)
    }
}
