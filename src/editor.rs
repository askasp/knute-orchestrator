use std::path::Path;

use anyhow::{Context, Result};

/// Open a file in $EDITOR (or $VISUAL, or vi as fallback).
/// This function blocks until the editor exits.
/// The caller must suspend the TUI before calling this and restore it after.
pub fn open_in_editor(file_path: &Path) -> Result<()> {
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string());

    let status = std::process::Command::new(&editor)
        .arg(file_path)
        .status()
        .context(format!("Failed to launch editor: {}", editor))?;

    if !status.success() {
        anyhow::bail!("Editor exited with status: {}", status);
    }

    Ok(())
}
