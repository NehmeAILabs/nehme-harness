use crate::filter::code_filter::{FilterLevel, Language, filter_source, strip_ansi};
use crate::filter::truncation::{
    CAP_ERRORS, CAP_INVENTORY, CAP_LIST, CAP_WARNINGS, MAX_LINE_LEN, clean_line, compact_path,
    reduced,
};

const INTENT_FILTER_THRESHOLD: usize = 5 * 1024;

pub fn compress_command_output_with_intent(
    command: &str,
    stdout: &str,
    stderr: &str,
    exit_code: i32,
    intent: Option<&str>,
) -> String {
    let combined = if stderr.is_empty() {
        stdout.to_string()
    } else if stdout.is_empty() {
        stderr.to_string()
    } else {
        format!("{}\n{}", stdout, stderr)
    };

    let combined = strip_ansi(&combined);

    if combined.is_empty() {
        return if exit_code != 0 {
            format!("Exit code: {}", exit_code)
        } else {
            "ok".to_string()
        };
    }

    if let Some(intent_str) = intent {
        if combined.len() > INTENT_FILTER_THRESHOLD {
            let filtered = intent_filter_compress(&combined, intent_str);
            if !filtered.is_empty() && filtered.len() < combined.len() / 2 {
                if exit_code != 0 && !filtered.contains(&format!("Exit code: {}", exit_code)) {
                    return format!("{}\nExit code: {}", filtered, exit_code);
                }
                return filtered;
            }
        }
    }

    compress_command_output(command, stdout, stderr, exit_code)
}

fn intent_filter_compress(output: &str, intent: &str) -> String {
    let mut relevant = Vec::new();
    let intent_lower = intent.to_lowercase();
    let keywords: Vec<&str> = intent_lower.split_whitespace().collect();

    for line in output.lines() {
        let line_lower = line.to_lowercase();
        if keywords.iter().any(|kw| line_lower.contains(kw)) {
            relevant.push(line.to_string());
        }
    }

    if relevant.is_empty() {
        let max_chars = 3000;
        output.chars().take(max_chars).collect()
    } else {
        relevant.join("\n")
    }
}

pub fn compress_command_output(
    command: &str,
    stdout: &str,
    stderr: &str,
    exit_code: i32,
) -> String {
    let cmd = command.trim();
    let cmd_lower = cmd.to_lowercase();
    let cmd_parts: Vec<&str> = cmd_lower.split_whitespace().collect();

    let combined = if stderr.is_empty() {
        stdout.to_string()
    } else if stdout.is_empty() {
        stderr.to_string()
    } else {
        format!("{}\n{}", stdout, stderr)
    };

    let combined = strip_ansi(&combined);

    if combined.is_empty() {
        return if exit_code != 0 {
            format!("Exit code: {}", exit_code)
        } else {
            "ok".to_string()
        };
    }

    if let Some(result) = try_compress_source_code(cmd, &cmd_parts, &combined, exit_code) {
        let result = if exit_code != 0 && !result.contains(&format!("Exit code: {}", exit_code)) {
            format!("{}\nExit code: {}", result, exit_code)
        } else {
            result
        };
        return result;
    }

    let compressed = if let Some(result) = try_compress_git(cmd, &cmd_parts, &combined, exit_code) {
        result
    } else if let Some(result) = try_compress_cargo(cmd, &cmd_parts, &combined, exit_code) {
        result
    } else if let Some(result) = try_compress_go_test(cmd, &cmd_parts, &combined, exit_code) {
        result
    } else if let Some(result) = try_compress_pytest(cmd, &cmd_parts, &combined, exit_code) {
        result
    } else if let Some(result) = try_compress_npm(cmd, &cmd_parts, &combined, exit_code) {
        result
    } else if let Some(result) = try_compress_docker(cmd, &cmd_parts, &combined, exit_code) {
        result
    } else if let Some(result) = try_compress_find(cmd, &cmd_parts, &combined, exit_code) {
        result
    } else if let Some(result) = try_compress_ls(cmd, &cmd_parts, &combined, exit_code) {
        result
    } else if let Some(result) = try_compress_generic(&combined, exit_code) {
        result
    } else {
        combined
    };

    if exit_code != 0 && !compressed.contains(&format!("Exit code: {}", exit_code)) {
        format!("{}\nExit code: {}", compressed, exit_code)
    } else {
        compressed
    }
}

fn try_compress_source_code(
    _cmd: &str,
    parts: &[&str],
    output: &str,
    exit_code: i32,
) -> Option<String> {
    let is_cat = parts.first() == Some(&"cat")
        || parts.first() == Some(&"head")
        || parts.first() == Some(&"tail")
        || parts.first() == Some(&"bat")
        || parts.first() == Some(&"less");
    let is_type =
        parts.first() == Some(&"type") && parts.len() > 1 && parts.get(1) != Some(&"python");
    if !is_cat && !is_type {
        return None;
    }

    let file_arg = parts.iter().rev().find(|p| !p.starts_with('-')).copied();
    let ext = file_arg.and_then(|f| {
        let name = f.rsplit('/').next().unwrap_or(f);
        name.rsplit('.').next()
    });

    let lang = ext
        .map(Language::from_extension)
        .unwrap_or(Language::Unknown);
    if lang == Language::Data || lang == Language::Unknown {
        return None;
    }

    if exit_code != 0 {
        return None;
    }

    let filtered = filter_source(output, &lang, FilterLevel::Minimal);
    if filtered.len() < output.len() {
        let saved_pct = ((output.len() - filtered.len()) * 100) / output.len().max(1);
        Some(format!("[source filtered: -{}%]\n{}", saved_pct, filtered))
    } else {
        None
    }
}

fn try_compress_git(_cmd: &str, parts: &[&str], output: &str, exit_code: i32) -> Option<String> {
    if parts.first() != Some(&"git") {
        return None;
    }

    match parts.get(1)? {
        &"status" => Some(compress_git_status(output)),
        &"log" => Some(compress_git_log(output)),
        &"diff" => Some(compress_git_diff(output)),
        &"show" => Some(compress_git_show(output)),
        &"add" | &"commit" | &"push" | &"pull" | &"fetch" => {
            Some(compress_git_simple(output, exit_code))
        }
        &"branch" => Some(compress_git_branch(output)),
        &"stash" => Some(compress_git_stash(output, exit_code)),
        &"merge" | &"rebase" => Some(compress_git_merge(output)),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GitStatusState {
    Normal,
    Rebase,
    MergeConflicts,
    MergeReady,
    CherryPick,
    Revert,
    Bisect,
    Am,
    Detached,
}

fn detect_status_state(output: &str) -> (GitStatusState, Option<&str>) {
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("HEAD detached") {
            let ref_name = trimmed
                .strip_prefix("HEAD detached at ")
                .or_else(|| trimmed.strip_prefix("HEAD detached from "));
            return (GitStatusState::Detached, ref_name);
        }
        if trimmed.contains("interactive rebase in progress")
            || trimmed.contains("rebase in progress")
        {
            return (GitStatusState::Rebase, None);
        }
        if trimmed.contains("You have unmerged paths") || trimmed.contains("merge conflicts") {
            return (GitStatusState::MergeConflicts, None);
        }
        if trimmed.contains("All conflicts fixed but you are still merging")
            || trimmed.contains("is still merging")
        {
            return (GitStatusState::MergeReady, None);
        }
        if trimmed.contains("You are currently cherry-picking") {
            return (GitStatusState::CherryPick, None);
        }
        if trimmed.contains("You are currently reverting") {
            return (GitStatusState::Revert, None);
        }
        if trimmed.contains("You are currently bisecting") {
            return (GitStatusState::Bisect, None);
        }
        if trimmed.starts_with("Last command done") && trimmed.contains("am") {
            return (GitStatusState::Am, None);
        }
    }
    (GitStatusState::Normal, None)
}

fn compress_git_status(output: &str) -> String {
    let (state, detached_ref) = detect_status_state(output);
    let mut staged = Vec::new();
    let mut unstaged = Vec::new();
    let mut untracked = Vec::new();
    let branch = output
        .lines()
        .find(|l| l.starts_with("On branch "))
        .map(|l| l.trim().trim_start_matches("On branch "));

    for line in output.lines() {
        if line.starts_with("On branch ") || line.starts_with("HEAD detached") || line.is_empty() {
            continue;
        }
        if line.starts_with('\t') || line.starts_with("  ") {
            let file = line.trim();
            if line.starts_with('\t') {
                unstaged.push(file);
            } else {
                untracked.push(file);
            }
        } else if line.len() >= 2 {
            let idx = if line.starts_with(' ') { 1 } else { 0 };
            let status = &line[..idx + 1];
            let file = line[idx + 1..].trim();
            if status.contains('M')
                || status.contains('A')
                || status.contains('D')
                || status.contains('R')
            {
                staged.push((status.trim(), file));
            }
        }
    }

    let mut result = String::new();

    match state {
        GitStatusState::Rebase => result.push_str("state: REBASE IN PROGRESS\n"),
        GitStatusState::MergeConflicts => result.push_str("state: MERGE CONFLICTS\n"),
        GitStatusState::MergeReady => {
            result.push_str("state: ALL CONFLICTS RESOLVED, STILL MERGING\n")
        }
        GitStatusState::CherryPick => result.push_str("state: CHERRY-PICK IN PROGRESS\n"),
        GitStatusState::Revert => result.push_str("state: REVERT IN PROGRESS\n"),
        GitStatusState::Bisect => result.push_str("state: BISECTING\n"),
        GitStatusState::Am => result.push_str("state: AM SESSION\n"),
        GitStatusState::Detached => {
            if let Some(r) = detached_ref {
                result.push_str(&format!("state: DETACHED at {}\n", r));
            } else {
                result.push_str("state: DETACHED HEAD\n");
            }
        }
        GitStatusState::Normal => {}
    }

    if let Some(b) = branch {
        result.push_str(&format!("branch: {}\n", b));
    }

    if !staged.is_empty() {
        let cap = CAP_LIST;
        result.push_str("staged:\n");
        for (status, file) in staged.iter().take(cap) {
            result.push_str(&format!("  {} {}\n", status, file));
        }
        if staged.len() > cap {
            result.push_str(&format!("  ... and {} more\n", staged.len() - cap));
        }
    }

    if !unstaged.is_empty() {
        let cap = CAP_LIST;
        result.push_str("unstaged:\n");
        for file in unstaged.iter().take(cap) {
            result.push_str(&format!("  {}\n", file));
        }
        if unstaged.len() > cap {
            result.push_str(&format!("  ... and {} more\n", unstaged.len() - cap));
        }
    }

    if !untracked.is_empty() {
        let cap = reduced(CAP_LIST, 5);
        result.push_str("untracked:\n");
        for file in untracked.iter().take(cap) {
            result.push_str(&format!("  {}\n", file));
        }
        if untracked.len() > cap {
            result.push_str(&format!("  ... and {} more\n", untracked.len() - cap));
        }
    }

    if result.is_empty() {
        "clean".to_string()
    } else {
        result.trim().to_string()
    }
}

fn compress_git_log(output: &str) -> String {
    let mut commits: Vec<String> = Vec::new();
    let mut current_body_lines: Vec<&str> = Vec::new();
    let mut in_body = false;
    let mut saw_separator = false;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "---END---" || trimmed == "---" {
            saw_separator = true;
            in_body = false;
            continue;
        }
        if trimmed.starts_with("commit ")
            || trimmed.starts_with("Merge:")
            || (!saw_separator && trimmed.contains(") "))
        {
            in_body = false;
            if !commits.is_empty() || trimmed.starts_with("commit ") || trimmed.contains(") ") {
                if commits.len() < CAP_LIST {
                    commits.push(trimmed.to_string());
                }
                in_body = true;
                current_body_lines.clear();
                continue;
            }
        }
        if in_body {
            let is_trailer = trimmed.starts_with("Signed-off-by:")
                || trimmed.starts_with("Co-authored-by:")
                || trimmed.starts_with("Reviewed-by:")
                || trimmed.starts_with("Acked-by:")
                || trimmed.starts_with("Tested-by:");
            if !is_trailer {
                current_body_lines.push(trimmed);
                if current_body_lines.len() <= 3 {
                    if let Some(last) = commits.last_mut() {
                        last.push_str(&format!("\n  {}", trimmed));
                    }
                }
            }
        }
    }

    if commits.is_empty() {
        let lines: Vec<&str> = output.lines().take(CAP_LIST).collect();
        let mut result = String::new();
        for line in &lines {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                result.push_str(trimmed);
                result.push('\n');
            }
        }
        let total = output.lines().count();
        if total > CAP_LIST {
            result.push_str(&format!("... and {} more commits", total - CAP_LIST));
        }
        if result.is_empty() {
            "no commits".to_string()
        } else {
            result.trim().to_string()
        }
    } else {
        let total = output.lines().filter(|l| !l.trim().is_empty()).count();
        let mut result = commits.join("\n");
        if total > CAP_LIST * 2 {
            result.push_str(&format!("\n... and more commits"));
        }
        result
    }
}

fn compress_git_diff(output: &str) -> String {
    let mut files: Vec<String> = Vec::new();
    let mut current_file_stats: Option<String> = None;
    let mut hunks: Vec<String> = Vec::new();
    let mut hunk_lines: usize = 0;
    let max_hunk_lines: usize = 40;

    for line in output.lines() {
        if line.starts_with("diff --git") {
            if let Some(stats) = current_file_stats.take() {
                files.push(stats);
            }
            hunks.clear();
            hunk_lines = 0;
            current_file_stats = Some(line.to_string());
        } else if line.starts_with("--- a/") || line.starts_with("+++ b/") {
            continue;
        } else if line.starts_with("@@") {
            hunks.push(line.to_string());
            hunk_lines = 0;
        } else if line.starts_with('+') || line.starts_with('-') {
            if !line.starts_with("---") && !line.starts_with("+++") {
                hunk_lines += 1;
                if hunk_lines <= max_hunk_lines {
                    hunks.push(line.to_string());
                } else if hunk_lines == max_hunk_lines + 1 {
                    hunks.push("  ...".to_string());
                }
            }
        }
    }

    if let Some(stats) = current_file_stats.take() {
        files.push(stats);
    }

    if files.is_empty() && hunks.is_empty() {
        if output.trim().is_empty() {
            return "no changes".to_string();
        }
        let lines: Vec<&str> = output.lines().take(CAP_LIST).collect();
        let mut result = lines.iter().map(|l| *l).collect::<Vec<&str>>().join("\n");
        let total = output.lines().count();
        if total > CAP_LIST {
            result.push_str(&format!("\n... and {} more lines", total - CAP_LIST));
        }
        return result;
    }

    let total_files = files.len();
    let files_to_show = files.into_iter().take(CAP_LIST).collect::<Vec<_>>();
    let mut result = files_to_show.join("\n");

    if !hunks.is_empty() {
        result.push('\n');
        result.push_str(
            &hunks
                .into_iter()
                .take(max_hunk_lines as usize)
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    if total_files > CAP_LIST {
        result.push_str(&format!("\n... and {} more files", total_files - CAP_LIST));
    }

    if result.is_empty() {
        "no changes".to_string()
    } else {
        result.trim().to_string()
    }
}

fn compress_git_show(output: &str) -> String {
    let mut result = String::new();
    let mut in_commit = true;
    let mut in_diff = false;
    let mut diff_lines: usize = 0;

    for line in output.lines() {
        let trimmed = line.trim();
        if in_commit {
            if trimmed.starts_with("diff --git") {
                in_commit = false;
                in_diff = true;
                diff_lines = 0;
                result.push('\n');
                result.push_str(trimmed);
                result.push('\n');
            } else if trimmed.starts_with("Date:") || trimmed.starts_with("Author:") {
                let parts: Vec<&str> = trimmed.splitn(2, ':').collect();
                if parts.len() == 2 {
                    result.push_str(parts[1].trim());
                    result.push(' ');
                }
            } else if !trimmed.is_empty() && !trimmed.starts_with("Merge:") {
                result.push_str(trimmed);
                result.push('\n');
            }
        } else if in_diff {
            diff_lines += 1;
            if diff_lines <= CAP_LIST {
                result.push_str(line);
                result.push('\n');
            } else if diff_lines == CAP_LIST + 1 {
                result.push_str("... and more diff lines\n");
            }
        }
    }

    if result.is_empty() {
        "no output".to_string()
    } else {
        result.trim().to_string()
    }
}

fn compress_git_simple(output: &str, exit_code: i32) -> String {
    let trimmed = output.trim();
    if exit_code == 0 {
        let hash = trimmed.lines().next().and_then(|l| {
            l.split_whitespace()
                .find(|w| w.len() == 7 && w.chars().all(|c| c.is_ascii_hexdigit()))
        });
        if let Some(h) = hash {
            return format!("ok {}", h);
        }
        "ok".to_string()
    } else {
        trimmed
            .lines()
            .take(CAP_ERRORS)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn compress_git_branch(output: &str) -> String {
    let lines: Vec<&str> = output.lines().take(CAP_LIST).collect();
    let mut result = lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let total = output.lines().count();
    if total > CAP_LIST {
        result.push_str(&format!("\n... and {} more", total - CAP_LIST));
    }
    if result.is_empty() {
        "no branches".to_string()
    } else {
        result
    }
}

fn compress_git_stash(output: &str, _exit_code: i32) -> String {
    let trimmed = output.trim();
    if trimmed.is_empty() || trimmed == "No stash entries found." {
        return "no stash entries".to_string();
    }
    let lines: Vec<&str> = trimmed.lines().collect();
    let total = lines.len();
    let shown: Vec<&str> = lines.iter().take(CAP_LIST).copied().collect();
    let mut result = shown.join("\n");
    if total > CAP_LIST {
        result.push_str(&format!(
            "\n... and {} more stash entries",
            total - CAP_LIST
        ));
    }
    result
}

fn compress_git_merge(output: &str) -> String {
    let mut result = String::new();
    let mut conflict_files = Vec::new();
    let mut in_conflicts = false;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("CONFLICT") || trimmed.starts_with("Merge conflict") {
            conflict_files.push(trimmed);
        } else if trimmed.starts_with("Automatic merge failed") || trimmed.starts_with("CONFLICT") {
            in_conflicts = true;
        } else if !in_conflicts && !trimmed.is_empty() {
            result.push_str(trimmed);
            result.push('\n');
        }
    }

    if !conflict_files.is_empty() {
        result.push_str("conflicts:\n");
        for cf in conflict_files.iter().take(CAP_ERRORS) {
            result.push_str(&format!("  {}\n", cf));
        }
    }

    if result.is_empty() {
        "ok".to_string()
    } else {
        result.trim().to_string()
    }
}

fn try_compress_cargo(_cmd: &str, parts: &[&str], output: &str, exit_code: i32) -> Option<String> {
    if parts.first() != Some(&"cargo") {
        return None;
    }

    match parts.get(1)? {
        &"test" => Some(compress_cargo_test(output, exit_code)),
        &"build" | &"check" | &"clippy" | &"run" => Some(compress_cargo_build(output, exit_code)),
        &"add" | &"rm" => Some(compress_cargo_simple(output, exit_code)),
        _ => None,
    }
}

fn compress_cargo_test(output: &str, exit_code: i32) -> String {
    let mut test_results = Vec::new();
    let mut failures = Vec::new();
    let mut summary = String::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("test ") && trimmed.contains("... ") {
            test_results.push(trimmed);
        } else if trimmed.starts_with("FAILED") || trimmed.contains("FAILED") {
            failures.push(trimmed);
        } else if trimmed.starts_with("test result:") {
            summary = trimmed.to_string();
        }
    }

    if exit_code == 0 {
        if summary.is_empty() {
            "all tests passed".to_string()
        } else {
            summary
        }
    } else {
        let mut result = String::new();
        if !failures.is_empty() {
            result.push_str("FAILED:\n");
            for f in failures.iter().take(CAP_ERRORS) {
                result.push_str(&format!("  {}\n", f));
            }
        }
        let failed_tests: Vec<&&str> = test_results
            .iter()
            .filter(|t| t.contains("FAILED"))
            .take(CAP_ERRORS)
            .collect();
        if !failed_tests.is_empty() {
            result.push_str("failed tests:\n");
            for t in failed_tests {
                result.push_str(&format!("  {}\n", t));
            }
        }
        if !summary.is_empty() {
            result.push_str(&summary);
        }
        if result.is_empty() {
            output
                .lines()
                .take(CAP_ERRORS)
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            result.trim().to_string()
        }
    }
}

fn compress_cargo_build(output: &str, exit_code: i32) -> String {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut compiling = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("error") {
            errors.push(trimmed);
        } else if trimmed.starts_with("warning") {
            if warnings.len() < CAP_WARNINGS {
                warnings.push(trimmed);
            }
        } else if trimmed.starts_with("Compiling") {
            compiling.push(trimmed);
        }
    }

    let mut result = String::new();
    if !compiling.is_empty() {
        result.push_str(&format!("compiled {} crates", compiling.len()));
        if compiling.len() <= 5 {
            result.push_str(": ");
            result.push_str(&compiling.join(", "));
        }
        result.push('\n');
    }

    if !errors.is_empty() {
        result.push_str(&format!("errors ({}):\n", errors.len()));
        for e in errors.iter().take(CAP_ERRORS) {
            result.push_str(&format!("  {}\n", e));
        }
        if errors.len() > CAP_ERRORS {
            result.push_str(&format!("  ... and {} more\n", errors.len() - CAP_ERRORS));
        }
    }

    if !warnings.is_empty() && exit_code == 0 {
        result.push_str(&format!("warnings ({}):\n", warnings.len()));
        for w in warnings.iter().take(CAP_WARNINGS) {
            result.push_str(&format!("  {}\n", w));
        }
    }

    if result.is_empty() {
        if exit_code == 0 {
            "ok".to_string()
        } else {
            output
                .lines()
                .take(CAP_ERRORS)
                .collect::<Vec<_>>()
                .join("\n")
        }
    } else {
        result.trim().to_string()
    }
}

fn compress_cargo_simple(output: &str, exit_code: i32) -> String {
    if exit_code == 0 {
        "ok".to_string()
    } else {
        output
            .lines()
            .take(CAP_ERRORS)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn try_compress_go_test(
    _cmd: &str,
    parts: &[&str],
    output: &str,
    exit_code: i32,
) -> Option<String> {
    if parts.first() != Some(&"go") || parts.get(1) != Some(&"test") {
        return None;
    }

    let mut passed = 0u32;
    let mut failed = Vec::new();
    let mut skipped = 0u32;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("ok ") || trimmed.starts_with("PASS") {
            passed += 1;
        } else if trimmed.starts_with("FAIL") || trimmed.starts_with("--- FAIL:") {
            failed.push(trimmed);
        } else if trimmed.starts_with("skip") || trimmed.contains("SKIP") {
            skipped += 1;
        }
    }

    if exit_code == 0 && failed.is_empty() {
        Some(format!("passed: {}, skipped: {}", passed, skipped))
    } else {
        let mut result = format!("passed: {}, failed: {}", passed, failed.len());
        if !failed.is_empty() {
            result.push('\n');
            for f in failed.iter().take(CAP_ERRORS) {
                result.push_str(&format!("  {}\n", f));
            }
        }
        Some(result)
    }
}

fn try_compress_pytest(_cmd: &str, parts: &[&str], output: &str, exit_code: i32) -> Option<String> {
    if !parts
        .first()
        .map(|p| *p == "pytest" || *p == "python")
        .unwrap_or(false)
    {
        return None;
    }
    if parts.first() == Some(&"python") && parts.get(1) != Some(&"-m") {
        return None;
    }

    let mut failed = Vec::new();
    let mut summary_line = String::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("FAILED") {
            failed.push(trimmed);
        } else if trimmed.contains("passed")
            && (trimmed.contains("failed") || trimmed.contains("error"))
        {
            summary_line = trimmed.to_string();
        } else if trimmed.contains("passed") && !trimmed.contains("failed") {
            summary_line = trimmed.to_string();
        }
    }

    if exit_code == 0 {
        Some(if summary_line.is_empty() {
            "all tests passed".to_string()
        } else {
            summary_line
        })
    } else {
        let mut result = String::new();
        if !failed.is_empty() {
            result.push_str("FAILED:\n");
            for f in failed.iter().take(CAP_ERRORS) {
                result.push_str(&format!("  {}\n", f));
            }
        }
        if !summary_line.is_empty() {
            result.push_str(&summary_line);
        }
        Some(if result.is_empty() {
            output
                .lines()
                .take(CAP_ERRORS)
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            result.trim().to_string()
        })
    }
}

fn try_compress_npm(_cmd: &str, parts: &[&str], output: &str, exit_code: i32) -> Option<String> {
    if parts.first() != Some(&"npm")
        && parts.first() != Some(&"pnpm")
        && parts.first() != Some(&"yarn")
    {
        return None;
    }

    match parts.get(1)? {
        &"install" | &"i" | &"add" => Some(compress_npm_install(output, exit_code)),
        &"test" => Some(compress_npm_test(output, exit_code)),
        &"run" | &"start" | &"build" => Some(compress_npm_build(output, exit_code)),
        _ => None,
    }
}

fn compress_npm_install(output: &str, exit_code: i32) -> String {
    let added = output
        .lines()
        .filter(|l| l.trim().starts_with("added"))
        .count();
    let removed = output
        .lines()
        .filter(|l| l.trim().contains("removed"))
        .count();
    if exit_code == 0 {
        let mut parts = Vec::new();
        if added > 0 {
            parts.push(format!("{} packages added", added));
        }
        if removed > 0 {
            parts.push(format!("{} removed", removed));
        }
        if parts.is_empty() {
            "ok".to_string()
        } else {
            parts.join(", ")
        }
    } else {
        output
            .lines()
            .take(CAP_ERRORS)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn compress_npm_test(output: &str, exit_code: i32) -> String {
    let mut failures = Vec::new();
    let mut summary = String::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.contains("FAIL") || trimmed.contains("✗") || trimmed.contains("×") {
            failures.push(trimmed);
        } else if trimmed.contains("Tests:")
            || trimmed.contains("test") && trimmed.contains("passed")
        {
            summary = trimmed.to_string();
        }
    }

    if exit_code == 0 {
        if summary.is_empty() {
            "all tests passed".to_string()
        } else {
            summary
        }
    } else {
        let mut result = String::new();
        if !failures.is_empty() {
            result.push_str(&format!("failed ({}):\n", failures.len()));
            for f in failures.iter().take(CAP_ERRORS) {
                result.push_str(&format!("  {}\n", f));
            }
        }
        if !summary.is_empty() {
            result.push_str(&summary);
        }
        if result.is_empty() {
            output
                .lines()
                .take(CAP_ERRORS)
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            result.trim().to_string()
        }
    }
}

fn compress_npm_build(output: &str, exit_code: i32) -> String {
    if exit_code == 0 {
        let last_line = output.lines().last().map(|l| l.trim()).unwrap_or("ok");
        last_line.to_string()
    } else {
        let mut errors: Vec<&str> = output
            .lines()
            .filter(|l| {
                let t = l.trim();
                t.starts_with("Error") || t.starts_with("error") || t.contains("ERR!")
            })
            .take(CAP_ERRORS)
            .collect();
        if errors.is_empty() {
            errors = output.lines().take(CAP_ERRORS).collect();
        }
        errors.join("\n")
    }
}

fn try_compress_docker(_cmd: &str, parts: &[&str], output: &str, exit_code: i32) -> Option<String> {
    if parts.first() != Some(&"docker") {
        return None;
    }

    match parts.get(1)? {
        &"ps" => Some(compress_docker_ps(output)),
        &"images" => Some(compress_docker_images(output)),
        &"logs" => Some(compress_docker_logs(output, exit_code)),
        _ => None,
    }
}

fn compress_docker_ps(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() {
        return "no containers".to_string();
    }

    let header = lines[0];
    let mut result = header.to_string();
    result.push('\n');

    let count = lines.len() - 1;
    for line in lines.iter().skip(1).take(CAP_LIST) {
        let fields: Vec<&str> = line.split_whitespace().take(4).collect();
        result.push_str(&fields.join("  "));
        result.push('\n');
    }
    if count > CAP_LIST {
        result.push_str(&format!("... and {} more", count - CAP_LIST));
    }
    result.trim().to_string()
}

fn compress_docker_images(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() {
        return "no images".to_string();
    }

    let header = lines[0];
    let mut result = header.to_string();
    result.push('\n');

    let count = lines.len() - 1;
    for line in lines.iter().skip(1).take(CAP_INVENTORY) {
        result.push_str(line.trim());
        result.push('\n');
    }
    if count > CAP_INVENTORY {
        result.push_str(&format!("... and {} more", count - CAP_INVENTORY));
    }
    result.trim().to_string()
}

fn compress_docker_logs(output: &str, exit_code: i32) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() {
        return "no output".to_string();
    }

    if exit_code != 0 {
        let errors: Vec<&&str> = lines
            .iter()
            .filter(|l| {
                let t = l.trim();
                t.contains("error")
                    || t.contains("Error")
                    || t.contains("FATAL")
                    || t.contains("panic")
            })
            .take(CAP_ERRORS)
            .collect();
        if !errors.is_empty() {
            return errors.iter().map(|l| **l).collect::<Vec<&str>>().join("\n");
        }
        return lines
            .iter()
            .rev()
            .take(CAP_ERRORS)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
    }

    let mut tail: Vec<String> = lines
        .iter()
        .rev()
        .take(10)
        .map(|l| (*l).to_string())
        .collect::<Vec<_>>();
    tail.reverse();
    if lines.len() > 10 {
        tail.insert(0, format!("... {} lines omitted ...", lines.len() - 10));
    }
    tail.join("\n")
}

fn try_compress_find(_cmd: &str, parts: &[&str], output: &str, _exit_code: i32) -> Option<String> {
    let is_find = parts.first() == Some(&"find")
        || parts.first() == Some(&"fd")
        || (parts.first() == Some(&"rg") && parts.contains(&"--files"))
        || (parts.first() == Some(&"grep") && parts.contains(&"-l"))
        || (parts.first() == Some(&"grep") && parts.contains(&"--files-with-matches"));
    if !is_find {
        return None;
    }

    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() {
        return Some("no files found".to_string());
    }

    let mut dirs: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    let mut exts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();

    for line in &lines {
        let path = line.trim();
        if path.is_empty() {
            continue;
        }
        let compacted = compact_path(path);

        if let Some(pos) = path.rfind('/') {
            let dir = compact_path(&path[..pos]);
            dirs.entry(dir).or_default().push(compacted);
        } else {
            dirs.entry(".".to_string()).or_default().push(compacted);
        }

        let fname = path.rsplit('/').next().unwrap_or(path);
        let ext = fname.rsplit('.').next().unwrap_or("");
        if !ext.is_empty() && ext.len() <= 10 && ext != fname {
            *exts.entry(ext.to_string()).or_insert(0) += 1;
        }
    }

    let total = lines.len();
    let mut result = format!("{} files found\n", total);

    if dirs.len() <= 10 {
        for (dir, files) in &dirs {
            if files.len() <= 5 {
                result.push_str(&format!("  {}/: {}\n", dir, files.join(", ")));
            } else {
                result.push_str(&format!("  {}/: {} files\n", dir, files.len()));
            }
        }
    } else {
        for (dir, files) in dirs.iter().take(10) {
            result.push_str(&format!("  {}/: {} files\n", dir, files.len()));
        }
        result.push_str(&format!("  ... and {} more directories\n", dirs.len() - 10));
    }

    if !exts.is_empty() {
        let mut ext_vec: Vec<(String, usize)> = exts.into_iter().collect();
        ext_vec.sort_by(|a, b| b.1.cmp(&a.1));
        let mut ext_summary = Vec::new();
        for (ext, count) in ext_vec.iter().take(8) {
            if *count > 1 {
                ext_summary.push(format!(".{}({})", ext, count));
            } else {
                ext_summary.push(format!(".{}", ext));
            }
        }
        result.push_str(&format!("extensions: {}\n", ext_summary.join(", ")));
    }

    Some(result.trim().to_string())
}

fn try_compress_ls(_cmd: &str, parts: &[&str], output: &str, _exit_code: i32) -> Option<String> {
    if parts.first() != Some(&"ls") && parts.first() != Some(&"dir") {
        return None;
    }

    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() {
        return None;
    }

    let total = lines.len();
    if total <= CAP_INVENTORY {
        let cleaned: Vec<String> = lines.iter().map(|l| clean_line(l, MAX_LINE_LEN)).collect();
        return Some(cleaned.join("\n"));
    }

    let mut files = 0usize;
    let mut dirs = 0usize;
    let mut exts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.ends_with('/') {
            dirs += 1;
        } else {
            files += 1;
            let fname = trimmed.rsplit('/').next().unwrap_or(trimmed);
            let ext = fname.rsplit('.').next().unwrap_or("");
            if !ext.is_empty() && ext.len() <= 10 && ext != fname {
                *exts.entry(ext.to_string()).or_insert(0) += 1;
            }
        }
    }

    let mut result = format!("{} entries ({} files, {} dirs)\n", total, files, dirs);
    let shown: Vec<&str> = lines.iter().take(CAP_LIST).copied().collect();
    for line in shown {
        result.push_str(&clean_line(line, MAX_LINE_LEN));
        result.push('\n');
    }
    if total > CAP_LIST {
        result.push_str(&format!("... and {} more\n", total - CAP_LIST));
    }

    if !exts.is_empty() {
        let mut ext_vec: Vec<(String, usize)> = exts.into_iter().collect();
        ext_vec.sort_by(|a, b| b.1.cmp(&a.1));
        let mut ext_summary = Vec::new();
        for (ext, count) in ext_vec.iter().take(8) {
            if *count > 1 {
                ext_summary.push(format!(".{}({})", ext, count));
            } else {
                ext_summary.push(format!(".{}", ext));
            }
        }
        result.push_str(&format!("extensions: {}\n", ext_summary.join(", ")));
    }

    Some(result.trim().to_string())
}

fn try_compress_generic(output: &str, exit_code: i32) -> Option<String> {
    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() {
        return Some(if exit_code == 0 {
            "ok".to_string()
        } else {
            format!("Exit code: {}", exit_code)
        });
    }

    if let Some(json_summary) = try_summarize_json(output) {
        return Some(json_summary);
    }

    if exit_code != 0 {
        let errors: Vec<&&str> = lines
            .iter()
            .filter(|l| {
                let t = l.to_lowercase();
                t.contains("error")
                    || t.contains("fatal")
                    || t.contains("panic")
                    || t.contains("exception")
            })
            .take(CAP_ERRORS)
            .collect();

        if !errors.is_empty() {
            let mut result = errors
                .iter()
                .map(|l| clean_line(*l, MAX_LINE_LEN))
                .collect::<Vec<_>>()
                .join("\n");
            let total_errors = lines
                .iter()
                .filter(|l| {
                    let t = l.to_lowercase();
                    t.contains("error")
                        || t.contains("fatal")
                        || t.contains("panic")
                        || t.contains("exception")
                })
                .count();
            if total_errors > CAP_ERRORS {
                result.push_str(&format!(
                    "\n... and {} more errors",
                    total_errors - CAP_ERRORS
                ));
            }
            return Some(result);
        }

        let tail: Vec<&str> = lines.iter().rev().take(CAP_ERRORS).copied().collect();
        return Some(
            tail.into_iter()
                .rev()
                .map(|l| clean_line(l, MAX_LINE_LEN))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    if lines.len() <= CAP_INVENTORY {
        return None;
    }

    let mut kept: Vec<String> = Vec::new();
    let mut kept_count = 0;
    let mut blank_run = 0;

    for line in &lines {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run <= 2 {
                kept.push((*line).to_string());
            }
        } else {
            blank_run = 0;
            kept.push(clean_line(line, MAX_LINE_LEN));
            kept_count += 1;
        }
        if kept_count >= CAP_LIST {
            break;
        }
    }

    if kept.len() < lines.len() {
        kept.push(format!("... {} more lines", lines.len() - kept.len()));
    }

    Some(kept.join("\n"))
}

fn try_summarize_json(output: &str) -> Option<String> {
    let trimmed = output.trim();
    if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
        return None;
    }
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
        match &val {
            serde_json::Value::Array(arr) => {
                let len = arr.len();
                if len <= CAP_LIST {
                    return None;
                }
                let mut result = format!("JSON array: {} items\n", len);
                let mut types: std::collections::BTreeMap<String, usize> =
                    std::collections::BTreeMap::new();
                for item in arr.iter().take(50) {
                    let t = match item {
                        serde_json::Value::Object(_) => "object",
                        serde_json::Value::Array(_) => "array",
                        serde_json::Value::String(_) => "string",
                        serde_json::Value::Number(_) => "number",
                        serde_json::Value::Bool(_) => "bool",
                        serde_json::Value::Null => "null",
                    };
                    *types.entry(t.to_string()).or_insert(0) += 1;
                }
                let type_summary: Vec<String> =
                    types.iter().map(|(k, v)| format!("{}({})", k, v)).collect();
                result.push_str(&format!("types: {}\n", type_summary.join(", ")));

                if let Some(first) = arr.first() {
                    if let serde_json::Value::Object(map) = first {
                        let keys: Vec<&String> = map.keys().take(8).collect();
                        result.push_str(&format!(
                            "fields: {}\n",
                            keys.iter()
                                .map(|k| k.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                }
                Some(result.trim().to_string())
            }
            serde_json::Value::Object(map) => {
                let mut result = format!("JSON object: {} keys\n", map.len());
                let keys: Vec<&String> = map.keys().take(10).collect();
                result.push_str(&format!(
                    "keys: {}\n",
                    keys.iter()
                        .map(|k| k.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                if map.len() > 10 {
                    result.push_str(&format!("... and {} more keys\n", map.len() - 10));
                }
                Some(result.trim().to_string())
            }
            _ => None,
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_git_status_clean() {
        let output = "On branch main\nnothing to commit, working tree clean\n";
        let result = compress_command_output("git status", output, "", 0);
        assert!(result.contains("clean") || result.contains("branch"));
    }

    #[test]
    fn test_compress_cargo_test_passing() {
        let output = "running 3 tests\ntest foo ... ok\ntest bar ... ok\ntest baz ... ok\n\ntest result: ok. 3 passed; 0 failed; 0 ignored; 0 measured";
        let result = compress_command_output("cargo test", output, "", 0);
        assert!(result.contains("passed") || result.contains("ok"));
    }

    #[test]
    fn test_compress_cargo_test_failing() {
        let output = "running 3 tests\ntest foo ... FAILED\ntest bar ... ok\ntest baz ... ok\n\nfailures:\n\n---- foo stdout ----\npanicked at 'assertion failed'\n\ntest result: FAILED. 1 passed; 1 failed; 0 ignored";
        let result = compress_command_output("cargo test", output, "", 1);
        assert!(result.contains("FAILED") || result.contains("failed"));
    }

    #[test]
    fn test_compress_cargo_build_success() {
        let output = "   Compiling serde v1.0.200\n   Compiling my-crate v0.1.0\n    Finished dev profile [unoptimized + debuginfo]";
        let result = compress_command_output("cargo build", output, "", 0);
        assert!(result.contains("compiled") || result.contains("ok"));
    }

    #[test]
    fn test_compress_cargo_build_error() {
        let output = "error[E0425]: cannot find value `x` in this scope\n  --> src/main.rs:5:9\n   |\n5 |     x\n   |     ^ not found in this scope\n\nFor more information about this error, try `rustc --explain E0425`.";
        let result = compress_command_output("cargo build", output, "", 1);
        assert!(result.contains("error"));
    }

    #[test]
    fn test_compress_unknown_command_no_compression_needed() {
        let output = "hello\nworld";
        let result = compress_command_output("echo hello", output, "", 0);
        assert_eq!(result, "hello\nworld");
    }

    #[test]
    fn test_compress_empty_success() {
        let result = compress_command_output("git add .", "", "", 0);
        assert_eq!(result, "ok");
    }

    #[test]
    fn test_compress_empty_failure() {
        let result = compress_command_output("git add .", "", "", 1);
        assert!(result.contains("Exit code: 1"));
    }

    #[test]
    fn test_compress_go_test_passing() {
        let output = "ok  pkg/example   0.123s\nok  pkg/other     0.456s";
        let result = compress_command_output("go test ./...", output, "", 0);
        assert!(result.contains("passed"));
    }

    #[test]
    fn test_compress_generic_long_output() {
        let output: String = (0..200)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let result = compress_command_output("some-command", &output, "", 0);
        assert!(result.contains("more lines") || result.len() < output.len());
    }

    #[test]
    fn test_git_status_rebase_state() {
        let output = "interactive rebase in progress; onto abc123\nOn branch feature\n\tmodified: src/main.rs\n\nno changes added to commit";
        let result = compress_command_output("git status", output, "", 0);
        assert!(result.contains("REBASE"));
    }

    #[test]
    fn test_git_status_detached() {
        let output = "HEAD detached at v1.0\nnothing to commit, working tree clean";
        let result = compress_command_output("git status", output, "", 0);
        assert!(result.contains("DETACHED"));
        assert!(result.contains("v1.0"));
    }

    #[test]
    fn test_git_status_merge_conflicts() {
        let output = "On branch main\nYou have unmerged paths.\n\tboth modified: src/lib.rs\n\nno changes added to commit";
        let result = compress_command_output("git status", output, "", 0);
        assert!(result.contains("MERGE CONFLICTS"));
    }

    #[test]
    fn test_git_log_compression() {
        let output = "abc1234 (HEAD -> main) feat: add new feature\n\nSome body text\n\nSigned-off-by: test@test.com\n\ndef5678 fix: bug fix\n\nAnother body line";
        let result = compress_command_output("git log", output, "", 0);
        assert!(result.contains("abc1234"));
        assert!(!result.contains("Signed-off-by"));
    }

    #[test]
    fn test_git_diff_compression() {
        let output = "diff --git a/src/main.rs b/src/main.rs\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,3 +1,4 @@\n fn main() {\n+    println!(\"hello\");\n }";
        let result = compress_command_output("git diff", output, "", 0);
        assert!(result.contains("diff --git"));
        assert!(result.contains("@@"));
    }

    #[test]
    fn test_git_show_compression() {
        let output = "commit abc1234\nAuthor: Test <test@test.com>\nDate:   Mon Jan 1 00:00:00 2024\n\n    feat: add feature\n\ndiff --git a/file.rs b/file.rs\n--- a/file.rs\n+++ b/file.rs\n@@ -1 +1,2 @@\n+new line";
        let result = compress_command_output("git show", output, "", 0);
        assert!(result.contains("abc1234"));
        assert!(result.contains("diff --git"));
    }

    #[test]
    fn test_git_stash() {
        let output =
            "stash@{0}: WIP on main: abc1234 some work\nstash@{1}: WIP on main: def5678 other work";
        let result = compress_command_output("git stash list", output, "", 0);
        assert!(result.contains("stash@{0}"));
    }

    #[test]
    fn test_git_stash_empty() {
        let result = compress_git_stash("", 0);
        assert!(result.contains("no stash"));
    }

    #[test]
    fn test_find_compression() {
        let output: String = (0..30)
            .map(|i| format!("src/module_{}/file_{}.rs", i / 5, i))
            .collect::<Vec<_>>()
            .join("\n");
        let result = compress_command_output("find . -name '*.rs'", &output, "", 0);
        assert!(result.contains("30 files found"));
        assert!(result.contains("extensions:"));
    }

    #[test]
    fn test_ls_compression() {
        let output: String = (0..100)
            .map(|i| format!("file_{}.rs", i))
            .collect::<Vec<_>>()
            .join("\n");
        let result = compress_command_output("ls", &output, "", 0);
        assert!(result.contains("entries"));
        assert!(result.contains("extensions:"));
    }

    #[test]
    fn test_json_array_summary() {
        let items: Vec<serde_json::Value> = (0..30)
            .map(|i| serde_json::json!({"id": i, "name": format!("item_{}", i)}))
            .collect();
        let json = serde_json::to_string(&items).unwrap();
        let result = compress_command_output("cat data.json", &json, "", 0);
        assert!(result.contains("30 items") || result.contains("JSON array"));
    }

    #[test]
    fn test_json_object_summary() {
        let mut map = serde_json::Map::new();
        for i in 0..15 {
            map.insert(format!("key_{}", i), serde_json::Value::Bool(true));
        }
        let json = serde_json::to_string(&serde_json::Value::Object(map)).unwrap();
        let result = compress_command_output("cat config.json", &json, "", 0);
        assert!(result.contains("15 keys"));
    }

    #[test]
    fn test_compact_path_short() {
        assert_eq!(compact_path("src/main.rs"), "src/main.rs");
    }

    #[test]
    fn test_compact_path_long() {
        let path = "a/b/c/d/e/f/g/h/file.rs";
        let compacted = compact_path(path);
        assert!(compacted.contains("..."));
        assert!(compacted.contains("file.rs"));
    }

    #[test]
    fn test_clean_line_short() {
        let line = "hello world";
        assert_eq!(clean_line(line, 300), line);
    }

    #[test]
    fn test_clean_line_long() {
        let line: String = "a".repeat(500);
        let cleaned = clean_line(&line, 300);
        assert!(cleaned.contains("..."));
        assert!(cleaned.len() < line.len());
    }
}
