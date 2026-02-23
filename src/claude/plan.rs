use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct AgentSpec {
    pub label: String,
    pub prompt: String,
}

#[derive(Debug, Clone)]
pub struct PlanResult {
    pub branch_name: String,
    pub agents: Vec<AgentSpec>,
}

#[derive(Deserialize)]
struct ClaudeJsonOutput {
    result: Option<String>,
}

#[derive(Deserialize)]
struct PlanJson {
    branch_name: String,
    agents: Vec<AgentJson>,
}

#[derive(Deserialize)]
struct AgentJson {
    label: String,
    prompt: String,
}

const PLANNING_PROMPT: &str = r#"You are a task planner. Break a user request into parallel tasks for AI coding agents sharing one git worktree.

Reply with ONLY this JSON (no markdown, no explanation, no code fences):
{"branch_name":"kebab-case-name","agents":[{"label":"short","prompt":"detailed task instructions"}]}

Rules: 1-5 agents, non-overlapping files, short kebab-case labels.

Request: "#;

pub async fn generate_plan(description: &str, context: Option<&str>, repo_root: &Path) -> Result<PlanResult> {
    let prompt = match context {
        Some(ctx) if !ctx.is_empty() => format!(
            "{}{}\n\nContext:\n{}", PLANNING_PROMPT, description, ctx
        ),
        _ => format!("{}{}", PLANNING_PROMPT, description),
    };

    let output = Command::new("claude")
        .arg("-p")
        .arg(&prompt)
        .arg("--output-format")
        .arg("json")
        .arg("--model")
        .arg("haiku")
        .current_dir(repo_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .context("Failed to run claude CLI")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        anyhow::bail!("claude error: {}", stderr.trim());
    }

    // Parse outer --output-format json wrapper
    let outer: ClaudeJsonOutput = serde_json::from_str(&stdout)
        .with_context(|| {
            format!(
                "Bad Claude JSON wrapper. First 300 chars: {}",
                &stdout[..stdout.len().min(300)]
            )
        })?;

    let result_text = outer
        .result
        .ok_or_else(|| anyhow::anyhow!("No 'result' field in Claude output"))?;

    // Try to extract JSON from Claude's response (it may include prose around it)
    let json_text = extract_json(&result_text)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No JSON object found in Claude response. Got: {}",
                &result_text[..result_text.len().min(300)]
            )
        })?;

    let plan: PlanJson = serde_json::from_str(json_text)
        .with_context(|| {
            format!(
                "JSON parse error. Extracted: {}",
                &json_text[..json_text.len().min(300)]
            )
        })?;

    if plan.agents.is_empty() {
        anyhow::bail!("Plan has no agents");
    }

    Ok(PlanResult {
        branch_name: plan.branch_name,
        agents: plan
            .agents
            .into_iter()
            .map(|a| AgentSpec {
                label: a.label,
                prompt: a.prompt,
            })
            .collect(),
    })
}

/// Find the first top-level JSON object `{...}` in a string,
/// handling nested braces correctly.
fn extract_json(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, ch) in s[start..].char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        match ch {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..start + i + 1]);
                }
            }
            _ => {}
        }
    }
    None
}
