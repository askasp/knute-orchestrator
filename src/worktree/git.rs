use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tokio::process::Command;

/// Sanitize a string into a valid git branch name.
fn sanitize_branch_name(name: &str) -> String {
    let mut result: String = name
        .chars()
        .map(|c| match c {
            ' ' | '\t' | '~' | '^' | ':' | '\\' | '*' | '?' | '[' => '-',
            c if c.is_ascii_control() => '-',
            c => c,
        })
        .collect();

    // Collapse consecutive hyphens
    while result.contains("--") {
        result = result.replace("--", "-");
    }

    // Strip leading/trailing hyphens and dots
    result = result.trim_matches(|c| c == '-' || c == '.').to_string();

    // Remove trailing .lock
    if result.ends_with(".lock") {
        result.truncate(result.len() - 5);
    }

    if result.is_empty() {
        result = "branch".to_string();
    }

    result
}

/// Create a new git worktree at `.knute/worktrees/<branch>` based on `base_branch`.
pub async fn create_worktree(
    repo_root: &Path,
    branch: &str,
    base_branch: &str,
) -> Result<PathBuf> {
    let branch = sanitize_branch_name(branch);
    let worktree_dir = repo_root.join(".knute").join("worktrees");
    tokio::fs::create_dir_all(&worktree_dir).await?;

    let worktree_path = worktree_dir.join(&branch);

    let output = Command::new("git")
        .args(["worktree", "add", "-b", &branch])
        .arg(&worktree_path)
        .arg(base_branch)
        .current_dir(repo_root)
        .output()
        .await
        .context("Failed to run git worktree add")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree add failed: {}", stderr.trim());
    }

    // Ensure .knute is in .gitignore
    ensure_gitignore(repo_root).await?;

    Ok(worktree_path)
}

/// Remove a git worktree.
pub async fn remove_worktree(repo_root: &Path, worktree_path: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["worktree", "remove", "--force"])
        .arg(worktree_path)
        .current_dir(repo_root)
        .output()
        .await
        .context("Failed to run git worktree remove")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree remove failed: {}", stderr.trim());
    }

    Ok(())
}

/// Get the full diff for the entire worktree.
pub async fn get_full_diff(worktree_path: &Path) -> Result<String> {
    let mut diff = String::new();

    // Staged changes (index vs HEAD, or all staged if no HEAD)
    let staged = Command::new("git")
        .args(["diff", "--cached"])
        .current_dir(worktree_path)
        .output()
        .await
        .context("Failed to run git diff --cached")?;
    let staged_diff = String::from_utf8_lossy(&staged.stdout);
    if !staged_diff.is_empty() {
        diff.push_str(&staged_diff);
    }

    // Unstaged changes (working tree vs index)
    let unstaged = Command::new("git")
        .args(["diff"])
        .current_dir(worktree_path)
        .output()
        .await
        .context("Failed to run git diff")?;
    let unstaged_diff = String::from_utf8_lossy(&unstaged.stdout);
    if !unstaged_diff.is_empty() {
        if !diff.is_empty() {
            diff.push('\n');
        }
        diff.push_str(&unstaged_diff);
    }

    // Untracked files
    let untracked = Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(worktree_path)
        .output()
        .await?;
    let untracked_stdout = String::from_utf8_lossy(&untracked.stdout);
    let untracked_files: Vec<&str> = untracked_stdout.lines().filter(|l| !l.is_empty()).collect();
    if !untracked_files.is_empty() {
        if !diff.is_empty() {
            diff.push('\n');
        }
        for f in &untracked_files {
            diff.push_str(&format!("new file: {}\n", f));
        }
    }

    Ok(diff)
}

/// Ensure `.knute` is in the repo's `.gitignore`.
async fn ensure_gitignore(repo_root: &Path) -> Result<()> {
    let gitignore_path = repo_root.join(".gitignore");
    let content = if gitignore_path.exists() {
        tokio::fs::read_to_string(&gitignore_path).await?
    } else {
        String::new()
    };

    if !content.lines().any(|line| line.trim() == ".knute") {
        let new_content = if content.is_empty() || content.ends_with('\n') {
            format!("{}.knute\n", content)
        } else {
            format!("{}\n.knute\n", content)
        };
        tokio::fs::write(&gitignore_path, new_content).await?;
    }

    Ok(())
}
