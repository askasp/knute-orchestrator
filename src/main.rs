mod app;
mod cli;
mod claude;
mod context;
mod editor;
mod event;
mod message;
mod model;
mod notes;
mod store;
mod terminal;
mod ui;
mod worktree;

use std::collections::{HashMap, HashSet};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use clap::Parser;
use portable_pty::PtySize;
use tokio::sync::mpsc;
use uuid::Uuid;

use message::{Action, Message};
use model::App;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    // Determine repo root
    let repo_root = match cli.repo {
        Some(path) => path.canonicalize().context("Invalid repo path")?,
        None => std::env::current_dir().context("Cannot determine current directory")?,
    };

    // Verify it's a git repo
    if !repo_root.join(".git").exists() {
        anyhow::bail!(
            "Not a git repository: {}. Run knute from inside a git repo.",
            repo_root.display()
        );
    }

    // Init terminal
    let mut tui = terminal::init().context("Failed to initialize terminal")?;

    // Two channels: UI events (keys/ticks) are never blocked by background work
    let (ui_tx, mut ui_rx) = mpsc::unbounded_channel::<Message>();
    let (bg_tx, mut bg_rx) = mpsc::unbounded_channel::<Message>();

    // Create app state
    let mut app = App::new(repo_root.clone());

    // Restore persisted sessions
    let restored = store::load_state(&repo_root);
    if !restored.is_empty() {
        let first_worktree_path = restored[0].worktree_path.clone();
        app.sessions = restored;
        app.selected_index = 0;
        app.sidebar_state.select(Some(0));
        app.mode = model::AppMode::WorktreeView {
            worktree_path: first_worktree_path,
        };
        app.sidebar_focused = true;
    }

    // Populate file cache for @autocomplete (git ls-files is fast)
    app.file_cache = load_file_cache(&repo_root).await;

    // Scan notes from .knute/notes/
    app.notes = notes::scan_notes(&repo_root);

    // Write permission hook script
    setup_permission_hook(&repo_root)?;

    // Write default MCP config (Figma) if not present
    setup_mcp_config(&repo_root);

    // Spawn event reader (keys → UI channel)
    let event_tx = ui_tx.clone();
    tokio::spawn(async move {
        event::run_event_reader(event_tx).await;
    });

    // Spawn tick timer (30fps → UI channel)
    let tick_tx = ui_tx.clone();
    tokio::spawn(async move {
        event::run_tick(tick_tx, 100).await;
    });

    // Track child processes for cleanup
    let mut child_processes: HashMap<Uuid, tokio::process::Child> = HashMap::new();
    let mut seen_requests: HashSet<String> = HashSet::new();
    let mut last_perm_scan = std::time::Instant::now();

    // Embedded terminal PTY handles (kept in main loop, not in App, to avoid Send issues)
    let mut pty_masters: HashMap<model::TerminalId, Box<dyn portable_pty::MasterPty + Send>> = HashMap::new();
    let mut pty_children: HashMap<model::TerminalId, Box<dyn portable_pty::Child + Send>> = HashMap::new();

    // Terminal output channel (separate from bg to avoid flooding)
    let (term_tx, mut term_rx) = mpsc::unbounded_channel::<Message>();

    // Main event loop
    let mut needs_redraw = true;
    let mut bg_dirty = false; // bg events arrived since last redraw
    loop {
        // Only render when needed (UI events, not bg terminal spam)
        if needs_redraw {
            tui.draw(|frame| ui::view(&mut app, frame))?;
            needs_redraw = false;
            bg_dirty = false;
        }

        // Wait for ANY message from any channel
        let first_msg = tokio::select! {
            msg = ui_rx.recv() => msg,
            msg = bg_rx.recv() => msg,
            msg = term_rx.recv() => msg,
        };
        let Some(msg) = first_msg else {
            break;
        };

        // UI events always trigger a redraw (except Tick — only if something changed)
        match &msg {
            Message::Key(_) | Message::MouseScroll { .. } | Message::Resize { .. } => {
                needs_redraw = true;
            }
            Message::Tick => {
                if bg_dirty {
                    needs_redraw = true;
                }
            }
            _ => {}
        }

        // Terminal output triggers redraw only when we're viewing that terminal
        if let Message::TerminalOutput { terminal_id, .. } = &msg {
            if let model::AppMode::EmbeddedTerminal { terminal_id: focused, .. } = &app.mode {
                if terminal_id == focused {
                    needs_redraw = true;
                }
            }
        }

        // Background events (Claude output, worktree results) mark dirty for next tick
        if matches!(&msg, Message::ClaudeOutput { .. } | Message::ClaudeStatsUpdate { .. }
            | Message::ClaudeSpawned { .. } | Message::ClaudeSpawnFailed { .. }
            | Message::ClaudeProcessExited { .. } | Message::WorktreeCreated { .. }
            | Message::WorktreeCreateFailed { .. } | Message::FullDiffLoaded { .. }
            | Message::GeneratePlanReceived { .. } | Message::GeneratePlanFailed { .. }
            | Message::PermissionRequested { .. }) {
            bg_dirty = true;
        }

        let mut all_actions = app.update(msg);

        // Drain pending UI events (keys, ticks)
        while let Ok(msg) = ui_rx.try_recv() {
            match &msg {
                Message::Key(_) | Message::MouseScroll { .. } | Message::Resize { .. } => {
                    needs_redraw = true;
                }
                Message::Tick => {
                    if bg_dirty {
                        needs_redraw = true;
                    }
                }
                _ => {}
            }
            all_actions.extend(app.update(msg));
        }

        // Drain terminal output (high-volume, only updates parser state)
        for _ in 0..500 {
            match term_rx.try_recv() {
                Ok(msg) => {
                    // Redraw if viewing the terminal that produced output
                    if let Message::TerminalOutput { terminal_id, .. } = &msg {
                        if let model::AppMode::EmbeddedTerminal { terminal_id: focused, .. } = &app.mode {
                            if terminal_id == focused {
                                needs_redraw = true;
                            }
                        }
                    }
                    if matches!(&msg, Message::TerminalExited { .. }) {
                        needs_redraw = true;
                    }
                    all_actions.extend(app.update(msg));
                }
                Err(_) => break,
            }
        }

        // Drain background events (Claude output, git results) — redraw on next tick
        for _ in 0..100 {
            match bg_rx.try_recv() {
                Ok(msg) => {
                    bg_dirty = true;
                    all_actions.extend(app.update(msg));
                }
                Err(_) => break,
            }
        }

        // Scan for new permission requests (throttled to every 500ms)
        if last_perm_scan.elapsed() >= std::time::Duration::from_millis(500) {
            if app.sessions.iter().any(|s| s.status == model::SessionStatus::Working) {
                last_perm_scan = std::time::Instant::now();
                for msg in scan_permission_requests(&repo_root, &mut seen_requests) {
                    all_actions.extend(app.update(msg));
                }
            }
        }

        // Execute side effects
        for action in all_actions {
            match action {
                Action::Quit => {
                    // Save state before quitting
                    store::save_state(&app);
                    // Kill all embedded terminals
                    for (_, mut child) in pty_children.drain() {
                        let _ = child.kill();
                    }
                    pty_masters.clear();
                    app.terminals.clear();
                    // Kill all child processes
                    for (_, mut child) in child_processes.drain() {
                        let _ = child.kill().await;
                    }
                    app.should_quit = true;
                }
                Action::CreateWorktree {
                    session_id,
                    repo_root,
                    branch,
                    base,
                } => {
                    let tx = bg_tx.clone();
                    tokio::spawn(async move {
                        match worktree::git::create_worktree(&repo_root, &branch, &base).await {
                            Ok(path) => {
                                let _ = tx.send(Message::WorktreeCreated { session_id, path });
                            }
                            Err(e) => {
                                let _ = tx.send(Message::WorktreeCreateFailed {
                                    session_id,
                                    error: e.to_string(),
                                });
                            }
                        }
                    });
                }
                Action::SpawnClaude {
                    session_id,
                    worktree_path,
                    prompt,
                    skip_permissions,
                } => {
                    if !skip_permissions {
                        write_claude_settings(&worktree_path, &repo_root);
                    }
                    let (resolved, _files) =
                        context::resolve_file_references(&prompt, &worktree_path);
                    let mcp_path = repo_root.join(".knute").join("mcp-config.json");
                    let mcp_opt = if mcp_path.exists() { Some(mcp_path.as_path()) } else { None };
                    spawn_claude_process(
                        session_id,
                        &worktree_path,
                        &resolved,
                        skip_permissions,
                        None,
                        mcp_opt,
                        bg_tx.clone(),
                        &mut child_processes,
                    )
                    .await;
                }
                Action::ResumeClaude {
                    session_id,
                    worktree_path,
                    claude_session_id,
                    message,
                    skip_permissions,
                } => {
                    if !skip_permissions {
                        write_claude_settings(&worktree_path, &repo_root);
                    }
                    let (resolved, _files) =
                        context::resolve_file_references(&message, &worktree_path);
                    let mcp_path = repo_root.join(".knute").join("mcp-config.json");
                    let mcp_opt = if mcp_path.exists() { Some(mcp_path.as_path()) } else { None };
                    spawn_claude_process(
                        session_id,
                        &worktree_path,
                        &resolved,
                        skip_permissions,
                        Some(&claude_session_id),
                        mcp_opt,
                        bg_tx.clone(),
                        &mut child_processes,
                    )
                    .await;
                }
                Action::KillProcess { session_id } => {
                    if let Some(mut child) = child_processes.remove(&session_id) {
                        let _ = child.kill().await;
                        let _ = bg_tx.send(Message::ClaudeProcessExited {
                            session_id,
                            exit_code: None,
                        });
                    }
                }
                Action::RemoveWorktree {
                    worktree_path,
                } => {
                    let repo = app.repo_root.clone();
                    tokio::spawn(async move {
                        let _ = worktree::git::remove_worktree(&repo, &worktree_path).await;
                    });
                }
                Action::LoadFullDiff {
                    worktree_path,
                } => {
                    let tx = bg_tx.clone();
                    tokio::spawn(async move {
                        match worktree::git::get_full_diff(&worktree_path).await {
                            Ok(diff) => {
                                let _ = tx.send(Message::FullDiffLoaded { worktree_path, diff });
                            }
                            Err(_) => {
                                let _ = tx.send(Message::FullDiffLoaded {
                                    worktree_path,
                                    diff: String::new(),
                                });
                            }
                        }
                    });
                }
                Action::GeneratePlan {
                    description,
                    context,
                    repo_root: plan_repo_root,
                    skip_permissions,
                } => {
                    let tx = bg_tx.clone();
                    tokio::spawn(async move {
                        // Resolve @file references from context for the planner
                        let resolved_context = context.as_deref().map(|ctx| {
                            let (resolved, _) = context::resolve_file_references(ctx, &plan_repo_root);
                            resolved
                        });
                        match claude::plan::generate_plan(&description, resolved_context.as_deref(), &plan_repo_root).await {
                            Ok(plan) => {
                                let agents = plan
                                    .agents
                                    .into_iter()
                                    .map(|a| (a.label, a.prompt))
                                    .collect();
                                let _ = tx.send(Message::GeneratePlanReceived {
                                    branch_name: plan.branch_name,
                                    agents,
                                    skip_permissions,
                                    context,
                                });
                            }
                            Err(e) => {
                                let _ = tx.send(Message::GeneratePlanFailed {
                                    error: e.to_string(),
                                });
                            }
                        }
                    });
                }
                Action::SpawnTerminal { worktree_path, command } => {
                    // Compute content area size: total minus sidebar (30) and status bar (1)
                    let term_size = tui.size().unwrap_or_default();
                    let content_cols = term_size.width.saturating_sub(30);
                    // Content area minus the hint line at the bottom of the terminal widget
                    let content_rows = term_size.height.saturating_sub(2);

                    let pty_system = portable_pty::native_pty_system();
                    let pair = pty_system.openpty(PtySize {
                        rows: content_rows,
                        cols: content_cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    });
                    let pair = match pair {
                        Ok(p) => p,
                        Err(e) => {
                            tracing::error!("Failed to open PTY: {}", e);
                            continue;
                        }
                    };

                    let mut cmd = if let Some(ref args) = command {
                        let mut c = portable_pty::CommandBuilder::new(&args[0]);
                        for arg in &args[1..] {
                            c.arg(arg);
                        }
                        c
                    } else {
                        let shell = std::env::var("SHELL").unwrap_or_else(|_| "bash".to_string());
                        portable_pty::CommandBuilder::new(&shell)
                    };
                    cmd.cwd(&worktree_path);

                    let child = match pair.slave.spawn_command(cmd) {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::error!("Failed to spawn shell: {}", e);
                            continue;
                        }
                    };
                    // Drop slave to avoid holding the PTY open
                    drop(pair.slave);

                    let writer = match pair.master.take_writer() {
                        Ok(w) => w,
                        Err(e) => {
                            tracing::error!("Failed to get PTY writer: {}", e);
                            continue;
                        }
                    };

                    // Assign terminal ID
                    let terminal_id = app.next_terminal_id;
                    app.next_terminal_id += 1;

                    // Label from command
                    let label = match &command {
                        Some(args) => args.first().map(|a| {
                            std::path::Path::new(a).file_name()
                                .unwrap_or_default().to_string_lossy().to_string()
                        }).unwrap_or_else(|| "shell".to_string()),
                        None => "shell".to_string(),
                    };

                    // Spawn async reader task (uses term_tx to avoid flooding bg channel)
                    let reader = pair.master.try_clone_reader();
                    let tx = term_tx.clone();
                    if let Ok(mut reader) = reader {
                        tokio::task::spawn_blocking(move || {
                            let mut buf = [0u8; 4096];
                            loop {
                                match reader.read(&mut buf) {
                                    Ok(0) => {
                                        let _ = tx.send(Message::TerminalExited { terminal_id });
                                        break;
                                    }
                                    Ok(n) => {
                                        let _ = tx.send(Message::TerminalOutput {
                                            terminal_id,
                                            data: buf[..n].to_vec(),
                                        });
                                    }
                                    Err(_) => {
                                        let _ = tx.send(Message::TerminalExited { terminal_id });
                                        break;
                                    }
                                }
                            }
                        });
                    }

                    let writer_handle = Arc::new(Mutex::new(writer));
                    app.mode = model::AppMode::EmbeddedTerminal {
                        worktree_path: worktree_path.clone(),
                        terminal_id,
                    };
                    app.sidebar_focused = false;
                    app.terminals.push(model::EmbeddedTerminalState {
                        id: terminal_id,
                        parser: vt100::Parser::new(content_rows, content_cols, 0),
                        writer: writer_handle,
                        worktree_path,
                        label,
                    });
                    pty_masters.insert(terminal_id, pair.master);
                    pty_children.insert(terminal_id, child);
                }
                Action::TerminalInput { terminal_id, data } => {
                    if let Some(term) = app.terminals.iter().find(|t| t.id == terminal_id) {
                        if let Ok(mut w) = term.writer.lock() {
                            let _ = w.write_all(&data);
                            let _ = w.flush();
                        }
                    }
                }
                Action::DetachTerminal => {
                    // Just switch mode back — terminal keeps running
                    if let model::AppMode::EmbeddedTerminal { worktree_path, .. } = &app.mode {
                        let wt = worktree_path.clone();
                        app.worktree_view.active_tab = model::WorktreeTab::Terminals;
                        app.mode = model::AppMode::WorktreeView { worktree_path: wt };
                        app.sidebar_focused = false;
                        // Rescan notes in case an editor was running
                        app.notes = notes::scan_notes(&repo_root);
                    }
                }
                Action::KillTerminal { terminal_id } => {
                    if let Some(mut child) = pty_children.remove(&terminal_id) {
                        let _ = child.kill();
                    }
                    pty_masters.remove(&terminal_id);
                    app.terminals.retain(|t| t.id != terminal_id);
                    // If we were focused on this terminal, return to Terminals tab
                    if let model::AppMode::EmbeddedTerminal { terminal_id: tid, worktree_path } = &app.mode {
                        if *tid == terminal_id {
                            let wt = worktree_path.clone();
                            app.worktree_view.active_tab = model::WorktreeTab::Terminals;
                            app.mode = model::AppMode::WorktreeView { worktree_path: wt };
                            app.sidebar_focused = false;
                        }
                    }
                }
                Action::DeleteNote { note_index } => {
                    if note_index < app.notes.len() {
                        let path = app.notes[note_index].path.clone();
                        let _ = notes::delete_note(&path);
                        app.notes = notes::scan_notes(&repo_root);
                        app.sidebar_focused = true;
                        let groups = app.worktree_groups();
                        if let Some(group) = groups.first() {
                            app.mode = model::AppMode::WorktreeView {
                                worktree_path: group.worktree_path.clone(),
                            };
                        } else {
                            app.mode = model::AppMode::NewSession;
                        }
                    }
                }
                Action::WritePermissionResponse {
                    request_path,
                    request_id,
                    claude_session_id: _,
                    allow,
                } => {
                    write_permission_response(&request_path, &request_id, allow);
                }
                Action::SaveState => {
                    store::save_state(&app);
                }
            }
        }

        // Clean up PTY handles for terminals that were removed (natural exit)
        let live_ids: std::collections::HashSet<model::TerminalId> =
            app.terminals.iter().map(|t| t.id).collect();
        let dead_ids: Vec<model::TerminalId> = pty_children.keys()
            .filter(|id| !live_ids.contains(id))
            .copied().collect();
        let had_dead = !dead_ids.is_empty();
        for id in dead_ids {
            if let Some(mut child) = pty_children.remove(&id) {
                let _ = child.kill();
            }
            pty_masters.remove(&id);
        }
        if had_dead {
            app.notes = notes::scan_notes(&repo_root);
        }

        // Resize PTY if embedded terminal is active and terminal size changed
        if let model::AppMode::EmbeddedTerminal { terminal_id, .. } = &app.mode {
            if let (Some(master), Some(term)) = (
                pty_masters.get(terminal_id),
                app.terminals.iter_mut().find(|t| t.id == *terminal_id),
            ) {
                let term_size = tui.size().unwrap_or_default();
                let content_cols = term_size.width.saturating_sub(30);
                let content_rows = term_size.height.saturating_sub(2);
                let (parser_rows, parser_cols) = term.parser.screen().size();
                if content_cols != parser_cols || content_rows != parser_rows {
                    let _ = master.resize(PtySize {
                        rows: content_rows,
                        cols: content_cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    });
                    term.parser.set_size(content_rows, content_cols);
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    terminal::restore().context("Failed to restore terminal")?;

    Ok(())
}

async fn load_file_cache(repo_root: &std::path::Path) -> Vec<String> {
    let output = tokio::process::Command::new("git")
        .args(["ls-files"])
        .current_dir(repo_root)
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(|l| l.to_string())
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Creates `.knute/mcp-config.json` with a default Figma MCP server if it
/// doesn't already exist. Edit this file to add/remove MCP servers.
fn setup_mcp_config(repo_root: &Path) {
    let config_path = repo_root.join(".knute").join("mcp-config.json");
    if config_path.exists() {
        return;
    }

    let config = serde_json::json!({
        "mcpServers": {
            "figma": {
                "type": "http",
                "url": "https://mcp.figma.com/mcp"
            }
        }
    });

    let _ = std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&config).unwrap_or_default(),
    );
}

fn setup_permission_hook(repo_root: &Path) -> Result<()> {
    let bin_dir = repo_root.join(".knute").join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let hook_path = bin_dir.join("permission-hook.sh");

    let perm_dir = repo_root.join(".knute").join("permissions");
    let script = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail

# Read permission request from stdin
REQUEST=$(cat)

# Generate unique request ID
REQUEST_ID="req_$(date +%s)_$$"

# Permissions directory
PERM_DIR="{perm_dir}"
mkdir -p "$PERM_DIR"

# Write request file
REQUEST_FILE="$PERM_DIR/$REQUEST_ID.request.json"
echo "$REQUEST" > "$REQUEST_FILE"

# Wait for response (poll every 250ms, timeout 120s)
RESPONSE_FILE="$PERM_DIR/$REQUEST_ID.response.json"
TIMEOUT=480
COUNT=0

while [ ! -f "$RESPONSE_FILE" ]; do
    sleep 0.25
    COUNT=$((COUNT + 1))
    if [ $COUNT -ge $TIMEOUT ]; then
        rm -f "$REQUEST_FILE"
        echo '{{"hookSpecificOutput":{{"hookEventName":"PermissionRequest","decision":{{"behavior":"deny","reason":"Knute permission timeout"}}}}}}'
        exit 0
    fi
done

cat "$RESPONSE_FILE"
rm -f "$REQUEST_FILE" "$RESPONSE_FILE"
"#,
        perm_dir = perm_dir.display()
    );

    std::fs::write(&hook_path, script)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755))?;
    }

    Ok(())
}

fn write_claude_settings(worktree_path: &Path, repo_root: &Path) {
    let hook_path = repo_root
        .join(".knute")
        .join("bin")
        .join("permission-hook.sh");
    let settings_dir = worktree_path.join(".claude");
    let _ = std::fs::create_dir_all(&settings_dir);

    let settings = serde_json::json!({
        "hooks": {
            "PermissionRequest": [{
                "matcher": "",
                "hooks": [{
                    "type": "command",
                    "command": hook_path.to_string_lossy()
                }]
            }]
        }
    });

    let _ = std::fs::write(
        settings_dir.join("settings.json"),
        serde_json::to_string_pretty(&settings).unwrap_or_default(),
    );
}

fn scan_permission_requests(
    repo_root: &Path,
    seen_requests: &mut HashSet<String>,
) -> Vec<Message> {
    let perm_dir = repo_root.join(".knute").join("permissions");
    let mut messages = Vec::new();

    let entries = match std::fs::read_dir(&perm_dir) {
        Ok(e) => e,
        Err(_) => return messages,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let fname = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if !fname.ends_with(".request.json") {
            continue;
        }
        let request_id = fname.trim_end_matches(".request.json").to_string();

        if seen_requests.contains(&request_id) {
            continue;
        }
        seen_requests.insert(request_id.clone());

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let json: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let tool_name = json
            .get("toolName")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();

        let tool_input = json
            .get("toolInput")
            .map(|v| {
                if let Some(s) = v.as_str() {
                    s.to_string()
                } else {
                    summarize_tool_input(v)
                }
            })
            .unwrap_or_default();

        let claude_session_id = json
            .get("sessionId")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        messages.push(Message::PermissionRequested {
            claude_session_id,
            request_id,
            tool_name,
            tool_input,
            request_path: path,
        });
    }

    messages
}

fn summarize_tool_input(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(cmd) = map.get("command") {
                if let Some(s) = cmd.as_str() {
                    return s.to_string();
                }
            }
            if let Some(path) = map.get("file_path") {
                if let Some(s) = path.as_str() {
                    return s.to_string();
                }
            }
            if let Some(pattern) = map.get("pattern") {
                if let Some(s) = pattern.as_str() {
                    return format!("pattern: {}", s);
                }
            }
            serde_json::to_string(value).unwrap_or_default()
        }
        serde_json::Value::String(s) => s.clone(),
        _ => value.to_string(),
    }
}

fn write_permission_response(request_path: &Path, request_id: &str, allow: bool) {
    let response_path = request_path
        .parent()
        .unwrap_or(request_path)
        .join(format!("{}.response.json", request_id));

    let behavior = if allow { "allow" } else { "deny" };
    let response = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": {
                "behavior": behavior
            }
        }
    });

    let _ = std::fs::write(
        &response_path,
        serde_json::to_string(&response).unwrap_or_default(),
    );
}

async fn spawn_claude_process(
    session_id: Uuid,
    worktree_path: &PathBuf,
    prompt: &str,
    skip_permissions: bool,
    resume_session_id: Option<&str>,
    mcp_config_path: Option<&Path>,
    tx: mpsc::UnboundedSender<Message>,
    child_processes: &mut HashMap<Uuid, tokio::process::Child>,
) {
    match claude::process::spawn_claude(
        session_id,
        worktree_path,
        prompt,
        skip_permissions,
        resume_session_id,
        mcp_config_path,
        tx.clone(),
    )
    .await
    {
        Ok(child) => {
            child_processes.insert(session_id, child);
        }
        Err(e) => {
            let _ = tx.send(Message::ClaudeSpawnFailed {
                session_id,
                error: e.to_string(),
            });
        }
    }
}
