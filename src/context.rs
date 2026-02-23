use std::path::Path;

/// Parse `@path/to/file` references from a message, read the files,
/// and return an enriched prompt with file contents prepended.
pub fn resolve_file_references(message: &str, working_dir: &Path) -> (String, Vec<String>) {
    let mut files_included = Vec::new();
    let mut remaining_parts = Vec::new();
    let mut file_contents = Vec::new();

    for token in message.split_whitespace() {
        if let Some(path_str) = token.strip_prefix('@') {
            if path_str.is_empty() {
                remaining_parts.push(token.to_string());
                continue;
            }

            let file_path = if Path::new(path_str).is_absolute() {
                std::path::PathBuf::from(path_str)
            } else {
                working_dir.join(path_str)
            };

            match std::fs::read_to_string(&file_path) {
                Ok(content) => {
                    files_included.push(path_str.to_string());
                    file_contents.push(format!(
                        "<file path=\"{}\">\n{}\n</file>",
                        path_str, content
                    ));
                }
                Err(_) => {
                    // If file doesn't exist, keep the @reference as-is
                    // so the user can see it wasn't resolved
                    remaining_parts.push(token.to_string());
                }
            }
        } else {
            remaining_parts.push(token.to_string());
        }
    }

    let user_message = remaining_parts.join(" ");

    if file_contents.is_empty() {
        return (user_message, files_included);
    }

    let prompt = format!(
        "Here are files for context:\n\n{}\n\n{}",
        file_contents.join("\n\n"),
        user_message
    );

    (prompt, files_included)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_no_references() {
        let (prompt, files) = resolve_file_references("just a normal message", Path::new("/tmp"));
        assert_eq!(prompt, "just a normal message");
        assert!(files.is_empty());
    }

    #[test]
    fn test_file_reference() {
        let dir = std::env::temp_dir().join("knute_test_ctx");
        let _ = fs::create_dir_all(&dir);
        fs::write(dir.join("test.rs"), "fn main() {}").unwrap();

        let (prompt, files) = resolve_file_references("@test.rs fix the bug", &dir);
        assert!(prompt.contains("<file path=\"test.rs\">"));
        assert!(prompt.contains("fn main() {}"));
        assert!(prompt.contains("fix the bug"));
        assert_eq!(files, vec!["test.rs"]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_missing_file_kept_as_is() {
        let (prompt, files) =
            resolve_file_references("@nonexistent.rs do stuff", Path::new("/tmp"));
        assert_eq!(prompt, "@nonexistent.rs do stuff");
        assert!(files.is_empty());
    }
}
