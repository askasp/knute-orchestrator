use std::path::{Path, PathBuf};

use crate::model::Note;

/// Scan `.knute/notes/` for markdown files, returning them sorted by title.
pub fn scan_notes(repo_root: &Path) -> Vec<Note> {
    let notes_dir = repo_root.join(".knute").join("notes");
    if !notes_dir.exists() {
        return Vec::new();
    }

    let mut notes = Vec::new();
    collect_notes(&notes_dir, &notes_dir, &mut notes);
    notes.sort_by(|a, b| {
        a.folder
            .as_deref()
            .unwrap_or("")
            .cmp(b.folder.as_deref().unwrap_or(""))
            .then_with(|| a.title.cmp(&b.title))
    });
    notes
}

fn collect_notes(base: &Path, dir: &Path, notes: &mut Vec<Note>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_notes(base, &path, notes);
        } else if path.extension().map_or(false, |e| e == "md") {
            let title = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let folder = path
                .parent()
                .and_then(|p| p.strip_prefix(base).ok())
                .filter(|rel| !rel.as_os_str().is_empty())
                .map(|rel| rel.to_string_lossy().to_string());

            notes.push(Note {
                path,
                title,
                folder,
                scroll_offset: 0,
            });
        }
    }
}

/// Create a new note file. Returns the path to the created file.
pub fn create_note(repo_root: &Path, title: &str, folder: &str) -> std::io::Result<PathBuf> {
    let notes_dir = repo_root.join(".knute").join("notes");
    let dir = if folder.is_empty() {
        notes_dir
    } else {
        notes_dir.join(folder)
    };
    std::fs::create_dir_all(&dir)?;

    let filename = format!("{}.md", title);
    let path = dir.join(&filename);
    if !path.exists() {
        std::fs::write(&path, "")?;
    }
    Ok(path)
}

/// Delete a note file.
pub fn delete_note(path: &Path) -> std::io::Result<()> {
    std::fs::remove_file(path)
}

/// Read note content for display.
pub fn read_note_content(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}
