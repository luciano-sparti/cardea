use ratatui::style::Color;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitFileStatus {
    Modified,
    Untracked,
    Staged,
    Deleted,
    Renamed,
    Ignored,
}

impl GitFileStatus {
    pub fn badge(&self) -> &'static str {
        match self {
            GitFileStatus::Modified => "M",
            GitFileStatus::Untracked => "?",
            GitFileStatus::Staged => "+",
            GitFileStatus::Deleted => "D",
            GitFileStatus::Renamed => "R",
            GitFileStatus::Ignored => "!",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            GitFileStatus::Modified => Color::Yellow,
            GitFileStatus::Untracked => Color::Green,
            GitFileStatus::Staged => Color::Cyan,
            GitFileStatus::Deleted => Color::Red,
            GitFileStatus::Renamed => Color::Magenta,
            GitFileStatus::Ignored => Color::DarkGray,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct GitRepoStatus {
    pub root: PathBuf,
    pub branch: String,
    pub ahead: usize,
    pub behind: usize,
    pub statuses: HashMap<PathBuf, GitFileStatus>,
}

impl GitRepoStatus {
    pub fn summary_string(&self) -> String {
        if self.branch.is_empty() {
            return String::new();
        }
        let mut s = format!(" {}", self.branch);
        if self.ahead > 0 || self.behind > 0 {
            let mut sync = String::new();
            if self.ahead > 0 {
                sync.push_str(&format!("⇡{}", self.ahead));
            }
            if self.behind > 0 {
                if !sync.is_empty() {
                    sync.push(' ');
                }
                sync.push_str(&format!("⇣{}", self.behind));
            }
            s.push_str(&format!(" [{}]", sync));
        }

        let mut mod_count = 0;
        let mut untracked_count = 0;
        let mut staged_count = 0;
        for status in self.statuses.values() {
            match status {
                GitFileStatus::Modified => mod_count += 1,
                GitFileStatus::Untracked => untracked_count += 1,
                GitFileStatus::Staged => staged_count += 1,
                _ => {}
            }
        }
        let mut counts = Vec::new();
        if staged_count > 0 {
            counts.push(format!("+{}", staged_count));
        }
        if mod_count > 0 {
            counts.push(format!("M:{}", mod_count));
        }
        if untracked_count > 0 {
            counts.push(format!("?:{}", untracked_count));
        }
        if !counts.is_empty() {
            s.push_str(&format!(" ({})", counts.join(" ")));
        }
        s
    }
}

pub fn find_git_root(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };

    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

pub fn query_git_status(path: &Path) -> Option<GitRepoStatus> {
    let root = find_git_root(path)?;

    let output = Command::new("git")
        .args(["status", "--porcelain=v1", "-b"])
        .current_dir(&root)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut repo_status = GitRepoStatus {
        root: root.clone(),
        branch: String::new(),
        ahead: 0,
        behind: 0,
        statuses: HashMap::new(),
    };

    for line in text.lines() {
        if line.starts_with("##") {
            // ## branch...upstream [ahead 1, behind 2]
            let branch_part = line.trim_start_matches("##").trim();
            if let Some((b, rest)) = branch_part.split_once("...") {
                repo_status.branch = b.to_string();
                if let Some(pos) = rest.find('[') {
                    let brackets = &rest[pos + 1..rest.len().saturating_sub(1)];
                    for part in brackets.split(',') {
                        let part = part.trim();
                        if let Some(n) = part.strip_prefix("ahead ") {
                            repo_status.ahead = n.parse().unwrap_or(0);
                        } else if let Some(n) = part.strip_prefix("behind ") {
                            repo_status.behind = n.parse().unwrap_or(0);
                        }
                    }
                }
            } else {
                repo_status.branch = branch_part.split_whitespace().next().unwrap_or("").to_string();
            }
            continue;
        }

        if line.len() < 4 {
            continue;
        }

        let index_status = line.as_bytes()[0];
        let worktree_status = line.as_bytes()[1];
        let file_path_str = line[3..].trim();
        // Handle renamed: "old -> new"
        let actual_path_str = if let Some((_, new_path)) = file_path_str.split_once(" -> ") {
            new_path.trim()
        } else {
            file_path_str
        };

        let file_abs_path = root.join(actual_path_str.trim_matches('"'));

        let status = if index_status == b'?' && worktree_status == b'?' {
            GitFileStatus::Untracked
        } else if index_status == b'!' && worktree_status == b'!' {
            GitFileStatus::Ignored
        } else if index_status == b'A' || index_status == b'M' || index_status == b'R' {
            GitFileStatus::Staged
        } else if worktree_status == b'M' {
            GitFileStatus::Modified
        } else if worktree_status == b'D' || index_status == b'D' {
            GitFileStatus::Deleted
        } else {
            GitFileStatus::Modified
        };

        repo_status.statuses.insert(file_abs_path, status);
    }

    Some(repo_status)
}

pub fn get_git_diff(path: &Path) -> Option<String> {
    let parent = if path.is_file() {
        path.parent()?
    } else {
        path
    };

    let output = Command::new("git")
        .args(["diff", "HEAD", "--", path.to_str()?])
        .current_dir(parent)
        .output()
        .ok()?;

    if output.status.success() && !output.stdout.is_empty() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        // Try without HEAD (for untracked or staged)
        let output2 = Command::new("git")
            .args(["diff", "--", path.to_str()?])
            .current_dir(parent)
            .output()
            .ok()?;
        if output2.status.success() && !output2.stdout.is_empty() {
            Some(String::from_utf8_lossy(&output2.stdout).to_string())
        } else {
            None
        }
    }
}
