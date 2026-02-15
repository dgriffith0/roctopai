use std::fs;
use std::io;
use std::io::Read as _;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;

use crate::models::{SessionStates, SOCKET_PATH};

pub fn start_event_socket(states: SessionStates) -> io::Result<()> {
    let _ = fs::remove_file(SOCKET_PATH);
    let listener = UnixListener::bind(SOCKET_PATH)?;

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    let mut buf = String::new();
                    let _ = stream.read_to_string(&mut buf);
                    if let Ok(event) = serde_json::from_str::<serde_json::Value>(&buf) {
                        if let (Some(session), Some(status)) =
                            (event["session"].as_str(), event["status"].as_str())
                        {
                            if let Ok(mut states) = states.lock() {
                                states.insert(session.to_string(), status.to_string());
                            }
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });

    Ok(())
}

pub fn ensure_hook_script() -> std::result::Result<PathBuf, String> {
    let config_dir = dirs::home_dir()
        .ok_or("Could not find home directory")?
        .join(".config/roctopai");
    fs::create_dir_all(&config_dir).map_err(|e| format!("Failed to create config dir: {}", e))?;

    let script_path = config_dir.join("event-hook.sh");
    let script = format!(
        r#"#!/bin/bash
# Roctopai event hook - sends Claude session events to the Unix socket
STATUS="$1"
cat > /dev/null
SESSION=$(basename "$PWD" | grep -o 'issue-[0-9]*')
[ -z "$SESSION" ] && exit 0
SOCKET="{socket}"
[ -S "$SOCKET" ] || exit 0
printf '{{"session":"%s","status":"%s"}}\n' "$SESSION" "$STATUS" | nc -w1 -U "$SOCKET" 2>/dev/null
exit 0
"#,
        socket = SOCKET_PATH
    );
    fs::write(&script_path, &script).map_err(|e| format!("Failed to write hook script: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Failed to chmod hook script: {}", e))?;
    }

    Ok(script_path)
}

pub fn write_worktree_hook_config(
    worktree_path: &str,
    hook_script: &str,
) -> std::result::Result<(), String> {
    let claude_dir = format!("{}/.claude", worktree_path);
    fs::create_dir_all(&claude_dir).map_err(|e| format!("Failed to create .claude dir: {}", e))?;

    let settings_path = format!("{}/.claude/settings.local.json", worktree_path);

    // Read existing settings if present and merge
    let mut settings: serde_json::Value = if let Ok(data) = fs::read_to_string(&settings_path) {
        serde_json::from_str(&data).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let hook_config = serde_json::json!({
        "PreToolUse": [{"hooks": [{"type": "command", "command": format!("'{}' working", hook_script), "async": true}]}],
        "Stop": [{"hooks": [{"type": "command", "command": format!("'{}' waiting", hook_script)}]}],
        "Notification": [{"matcher": "idle_prompt", "hooks": [{"type": "command", "command": format!("'{}' waiting", hook_script), "async": true}]}]
    });

    settings["hooks"] = hook_config;

    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .map_err(|e| format!("Failed to write hook settings: {}", e))?;

    Ok(())
}
