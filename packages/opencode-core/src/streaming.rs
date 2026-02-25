use napi::Result;
use std::collections::HashMap;
use std::process::{Command, Stdio};

#[napi(object)]
#[derive(Clone)]
pub struct StreamConfig {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: HashMap<String, String>,
    pub timeout_ms: Option<u32>,
}

#[napi(object)]
#[derive(Clone)]
pub struct StreamChunk {
    pub data: String,
    pub stream_type: String,
    pub is_complete: bool,
    pub exit_code: Option<i32>,
}

#[napi]
pub fn stream_command(config: StreamConfig) -> Result<StreamChunk> {
    let mut cmd = Command::new(&config.command);
    cmd.args(&config.args)
        .current_dir(&config.cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    for (k, v) in &config.env {
        cmd.env(k, v);
    }
    let out = cmd
        .output()
        .map_err(|err| napi::Error::from_reason(err.to_string()))?;

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let data = if stderr.is_empty() {
        stdout.clone()
    } else if stdout.is_empty() {
        stderr.clone()
    } else {
        format!("{stdout}{stderr}")
    };

    let stream_type = if !stderr.is_empty() && stdout.is_empty() {
        "stderr".to_string()
    } else {
        "stdout".to_string()
    };

    Ok(StreamChunk {
        data,
        stream_type,
        is_complete: true,
        exit_code: out.status.code(),
    })
}
