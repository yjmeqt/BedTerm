//! Port of `PromptContext.swift`.
//!
//! Snapshot of the prompt-context chips shown above the block-list composer's
//! input. `host` falls out of the connection credential; `cwd` / `git_branch`
//! track the most recent `Precmd` event (mirrored observable on
//! `BlockStore.latestPwd` / `latestGitBranch`); `last_exit_code` is the
//! previous sealed block's exit.

#![allow(dead_code)]

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PromptContext {
    pub host: Option<String>,
    pub cwd: Option<String>,
    pub last_exit_code: Option<i32>,
    pub git_branch: Option<String>,
}

impl PromptContext {
    pub fn new(
        host: Option<String>,
        cwd: Option<String>,
        last_exit_code: Option<i32>,
        git_branch: Option<String>,
    ) -> Self {
        Self {
            host,
            cwd,
            last_exit_code,
            git_branch,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_all_none() {
        let p = PromptContext::default();
        assert!(p.host.is_none());
        assert!(p.cwd.is_none());
        assert!(p.last_exit_code.is_none());
        assert!(p.git_branch.is_none());
    }

    #[test]
    fn equality_by_value() {
        let a = PromptContext::new(
            Some("host".to_owned()),
            Some("/tmp".to_owned()),
            Some(0),
            Some("main".to_owned()),
        );
        let b = PromptContext::new(
            Some("host".to_owned()),
            Some("/tmp".to_owned()),
            Some(0),
            Some("main".to_owned()),
        );
        assert_eq!(a, b);
    }

    #[test]
    fn equality_distinguishes_fields() {
        let a = PromptContext::new(Some("h".to_owned()), None, None, None);
        let b = PromptContext::new(Some("g".to_owned()), None, None, None);
        assert_ne!(a, b);
    }
}
