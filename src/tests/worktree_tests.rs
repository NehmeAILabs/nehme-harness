#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::cli::Cli;
    use crate::config::Config;
    use crate::extras::git_worktree::*;

    #[test]
    fn test_worktree_info_clone() {
        let info = WorktreeInfo {
            branch: "feature-x".into(),
            worktree_path: PathBuf::from("/tmp/wt"),
            main_repo_path: PathBuf::from("/tmp/repo"),
        };
        let cloned = info.clone();
        assert_eq!(cloned.branch, "feature-x");
        assert_eq!(cloned.worktree_path, PathBuf::from("/tmp/wt"));
        assert_eq!(cloned.main_repo_path, PathBuf::from("/tmp/repo"));
    }

    #[test]
    fn test_merge_outcome_success_eq() {
        assert_eq!(MergeOutcome::Success, MergeOutcome::Success);
    }

    #[test]
    fn test_merge_outcome_conflicts_eq() {
        let a = MergeOutcome::Conflicts(vec!["a".into(), "b".into()]);
        let b = MergeOutcome::Conflicts(vec!["a".into(), "b".into()]);
        assert_eq!(a, b);
    }

    #[test]
    fn test_merge_outcome_conflicts_ne() {
        let a = MergeOutcome::Conflicts(vec!["a".into()]);
        let b = MergeOutcome::Conflicts(vec!["b".into()]);
        assert_ne!(a, b);
    }

    #[test]
    fn test_merge_outcome_error_eq() {
        let a = MergeOutcome::Error("msg".into());
        let b = MergeOutcome::Error("msg".into());
        assert_eq!(a, b);
    }

    #[test]
    fn test_merge_outcome_error_ne() {
        let a = MergeOutcome::Error("a".into());
        let b = MergeOutcome::Error("b".into());
        assert_ne!(a, b);
    }

    #[test]
    fn test_merge_outcome_cross_variant_ne() {
        assert_ne!(MergeOutcome::Success, MergeOutcome::Error("err".into()));
        assert_ne!(
            MergeOutcome::Success,
            MergeOutcome::Conflicts(vec!["f".into()])
        );
    }

    #[test]
    fn test_merge_state_clone() {
        let state = MergeState {
            info: WorktreeInfo {
                branch: "feat".into(),
                worktree_path: PathBuf::from("/tmp/wt"),
                main_repo_path: PathBuf::from("/tmp/repo"),
            },
            original_branch: "main".into(),
            orig_dir: PathBuf::from("/tmp/wt"),
            stashed: true,
        };
        let cloned = state.clone();
        assert_eq!(cloned.original_branch, "main");
        assert!(cloned.stashed);
        assert_eq!(cloned.orig_dir, PathBuf::from("/tmp/wt"));
        assert_eq!(cloned.info.branch, "feat");
    }

    #[test]
    fn test_repo_name_basic() {
        assert_eq!(
            repo_name(&PathBuf::from("/home/user/my-project")),
            "my-project"
        );
    }

    #[test]
    fn test_repo_name_trailing_slash() {
        assert_eq!(repo_name(&PathBuf::from("/home/user/repo/")), "repo");
    }

    #[test]
    fn test_repo_name_empty() {
        assert_eq!(repo_name(&PathBuf::from("")), "unknown");
    }

    #[test]
    fn test_repo_name_root() {
        assert_eq!(repo_name(&PathBuf::from("/")), "unknown");
    }

    #[test]
    fn test_wt_cli_flags_default() {
        let cli = Cli::default();
        assert!(cli.worktree.is_none());
        assert!(!cli.wt_auto_merge);
        assert!(!cli.parallel);
        assert!(cli.wt_base_dir.is_none());
        assert!(!cli.wt_force);
    }

    #[test]
    fn test_wt_cli_flags_enabled() {
        let cli = Cli {
            worktree: Some("feature-x".into()),
            wt_auto_merge: true,
            wt_force: true,
            wt_base_dir: Some("/tmp".into()),
            ..Default::default()
        };
        assert_eq!(cli.worktree.as_deref(), Some("feature-x"));
        assert!(cli.wt_auto_merge);
        assert!(cli.wt_force);
        assert_eq!(cli.wt_base_dir.as_deref(), Some("/tmp"));
    }

    #[test]
    fn test_resolve_wt_auto_merge_cli() {
        let cli = Cli {
            wt_auto_merge: true,
            ..Default::default()
        };
        let cfg = Config::default();
        assert!(cli.resolve_wt_auto_merge(&cfg));
    }

    #[test]
    fn test_resolve_wt_auto_merge_parallel() {
        let cli = Cli {
            parallel: true,
            ..Default::default()
        };
        let cfg = Config::default();
        assert!(cli.resolve_wt_auto_merge(&cfg));
    }

    #[test]
    fn test_resolve_wt_auto_merge_config() {
        let cli = Cli::default();
        let cfg = Config {
            wt_auto_merge: Some(true),
            ..Default::default()
        };
        assert!(cli.resolve_wt_auto_merge(&cfg));
    }

    #[test]
    fn test_resolve_wt_auto_merge_default_false() {
        let cli = Cli::default();
        let cfg = Config::default();
        assert!(!cli.resolve_wt_auto_merge(&cfg));
    }

    #[test]
    fn test_resolve_wt_force_cli() {
        let cli = Cli {
            wt_force: true,
            ..Default::default()
        };
        let cfg = Config::default();
        assert!(cli.resolve_wt_force(&cfg));
    }

    #[test]
    fn test_resolve_wt_force_config() {
        let cli = Cli::default();
        let cfg = Config {
            wt_force: Some(true),
            ..Default::default()
        };
        assert!(cli.resolve_wt_force(&cfg));
    }

    #[test]
    fn test_resolve_wt_force_default_false() {
        let cli = Cli::default();
        let cfg = Config::default();
        assert!(!cli.resolve_wt_force(&cfg));
    }

    #[test]
    fn test_resolve_wt_base_dir_cli() {
        let cli = Cli {
            wt_base_dir: Some("/custom/base".into()),
            ..Default::default()
        };
        let cfg = Config::default();
        assert_eq!(
            cli.resolve_wt_base_dir(&cfg),
            Some(PathBuf::from("/custom/base"))
        );
    }

    #[test]
    fn test_resolve_wt_base_dir_config() {
        let cli = Cli::default();
        let cfg = Config {
            wt_base_dir: Some("/config/base".into()),
            ..Default::default()
        };
        assert_eq!(
            cli.resolve_wt_base_dir(&cfg),
            Some(PathBuf::from("/config/base"))
        );
    }

    #[test]
    fn test_resolve_wt_base_dir_default_none() {
        let cli = Cli::default();
        let cfg = Config::default();
        assert_eq!(cli.resolve_wt_base_dir(&cfg), None);
    }

    #[test]
    fn test_resolve_wt_base_dir_cli_overrides_config() {
        let cli = Cli {
            wt_base_dir: Some("/cli".into()),
            ..Default::default()
        };
        let cfg = Config {
            wt_base_dir: Some("/config".into()),
            ..Default::default()
        };
        assert_eq!(cli.resolve_wt_base_dir(&cfg), Some(PathBuf::from("/cli")));
    }

    #[test]
    fn test_default_branch_is_refutable() {
        // Pure-logic: the function returns None for non-existent paths (no git init)
        assert!(default_branch(&PathBuf::from("/tmp/nonexistent_repo")).is_none());
    }
}
