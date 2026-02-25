use napi::Result;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{mpsc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Clone)]
struct Chunk {
    data: String,
    stream_type: String,
    sequence: u64,
    timestamp_ms: i64,
}

struct Session {
    child: Child,
    stdin_pipe: Option<ChildStdin>,
    rx: mpsc::Receiver<Chunk>,
    complete_sent: bool,
    started: Instant,
    timeout_ms: Option<u32>,
    reason: Option<String>,
    sequence: u64,
}

#[napi(object)]
#[derive(Clone)]
pub struct StreamConfig {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: HashMap<String, String>,
    pub timeout_ms: Option<u32>,
    pub stdin_mode: Option<String>,
    pub use_pty: Option<bool>,
}

#[napi(object)]
#[derive(Clone)]
pub struct StreamStartConfig {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: HashMap<String, String>,
    pub timeout_ms: Option<u32>,
    pub chunk_size: Option<u32>,
    pub stdin_mode: Option<String>,
    pub use_pty: Option<bool>,
}

#[napi(object)]
#[derive(Clone)]
pub struct StreamChunk {
    pub data: String,
    pub stream_type: String,
    pub is_complete: bool,
    pub exit_code: Option<i32>,
    pub complete_reason: Option<String>,
    pub sequence: Option<u32>,
    pub timestamp_ms: Option<i64>,
}

#[napi(object)]
#[derive(Clone)]
pub struct StreamSession {
    pub id: String,
    pub pid: u32,
}

#[napi(object)]
#[derive(Clone)]
pub struct StreamReadResult {
    pub chunks: Vec<StreamChunk>,
    pub is_complete: bool,
}

static SESSIONS: OnceLock<Mutex<HashMap<String, Session>>> = OnceLock::new();
static PTY_READY: OnceLock<bool> = OnceLock::new();

fn sessions() -> &'static Mutex<HashMap<String, Session>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn spawn_reader<T: Read + Send + 'static>(
    mut reader: T,
    tx: mpsc::Sender<Chunk>,
    stream_type: &str,
    chunk_size: usize,
    sequence_base: u64,
) {
    let stream_type = stream_type.to_string();
    thread::spawn(move || {
        let mut buf = vec![0u8; chunk_size.max(1024)];
        let mut sequence = sequence_base;
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    let _ = tx.send(Chunk {
                        data,
                        stream_type: stream_type.clone(),
                        sequence,
                        timestamp_ms: now_ms(),
                    });
                    sequence = sequence.saturating_add(2);
                }
                Err(_) => break,
            }
        }
    });
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|item| item.as_millis() as i64)
        .unwrap_or(0)
}

#[napi]
pub fn stream_start(config: StreamStartConfig) -> Result<StreamSession> {
    let chunk_size = config.chunk_size.unwrap_or(64 * 1024) as usize;
    let (tx, rx) = mpsc::channel::<Chunk>();
    let use_pty = config.use_pty.unwrap_or(false);
    let mut child = if use_pty && pty_ready() {
        let mut wrapped = Command::new("script");
        wrapped
            .arg("-q")
            .arg("/dev/null")
            .arg("-c")
            .arg(build_shell_command(&config.command, &config.args));
        wrapped
            .current_dir(&config.cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(stdin_from_mode(config.stdin_mode.as_deref()));
        for (k, v) in &config.env {
            wrapped.env(k, v);
        }
        match wrapped.spawn() {
            Ok(child) => child,
            Err(_) => spawn_direct(&config)?,
        }
    } else {
        spawn_direct(&config)?
    };
    let pid = child.id();
    let stdin_pipe = child.stdin.take();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| napi::Error::from_reason("missing stdout".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| napi::Error::from_reason("missing stderr".to_string()))?;
    spawn_reader(stdout, tx.clone(), "stdout", chunk_size, 0);
    spawn_reader(stderr, tx, "stderr", chunk_size, 1);
    let session = Session {
        child,
        stdin_pipe,
        rx,
        complete_sent: false,
        started: Instant::now(),
        timeout_ms: config.timeout_ms,
        reason: None,
        sequence: 2,
    };

    let id = Uuid::new_v4().to_string();
    sessions()
        .lock()
        .map_err(|_| napi::Error::from_reason("stream lock failed".to_string()))?
        .insert(id.clone(), session);
    Ok(StreamSession { id, pid })
}

#[napi]
pub fn stream_read(id: String, max_chunks: Option<u32>, wait_ms: Option<u32>) -> Result<StreamReadResult> {
    let mut session = {
        let mut store = sessions()
            .lock()
            .map_err(|_| napi::Error::from_reason("stream lock failed".to_string()))?;
        let Some(session) = store.remove(&id) else {
            return Ok(StreamReadResult {
                chunks: vec![],
                is_complete: true,
            });
        };
        session
    };

    if session.reason.is_none() {
        if let Some(limit) = session.timeout_ms {
            if session.started.elapsed() >= Duration::from_millis(limit as u64) {
                session.reason = Some("timeout".to_string());
                let _ = session.child.kill();
            }
        }
    }

    let limit = max_chunks.unwrap_or(64) as usize;
    let mut chunks = vec![];
    while chunks.len() < limit {
        let next = if chunks.is_empty() {
            if let Some(wait) = wait_ms {
                if wait > 0 {
                    match session.rx.recv_timeout(Duration::from_millis(wait as u64)) {
                        Ok(item) => Ok(item),
                        Err(mpsc::RecvTimeoutError::Timeout) => Err(mpsc::TryRecvError::Empty),
                        Err(mpsc::RecvTimeoutError::Disconnected) => Err(mpsc::TryRecvError::Disconnected),
                    }
                } else {
                    session.rx.try_recv()
                }
            } else {
                session.rx.try_recv()
            }
        } else {
            session.rx.try_recv()
        };

        match next {
            Ok(item) => chunks.push(StreamChunk {
                data: item.data,
                stream_type: item.stream_type,
                is_complete: false,
                exit_code: None,
                complete_reason: None,
                sequence: Some(item.sequence as u32),
                timestamp_ms: Some(item.timestamp_ms),
            }),
            Err(mpsc::TryRecvError::Empty) => break,
            Err(mpsc::TryRecvError::Disconnected) => break,
        }
    }

    if !session.complete_sent {
        if let Some(exit) = try_wait_process(&mut session.child)? {
            session.complete_sent = true;
            let reason = session.reason.clone().unwrap_or_else(|| "exit".to_string());
            let sequence = session.sequence;
            session.sequence = session.sequence.saturating_add(1);
            chunks.push(StreamChunk {
                data: String::new(),
                stream_type: "stdout".to_string(),
                is_complete: true,
                exit_code: Some(exit),
                complete_reason: Some(reason),
                sequence: Some(sequence as u32),
                timestamp_ms: Some(now_ms()),
            });
        }
    }

    let is_complete = session.complete_sent;
    if !is_complete {
        sessions()
            .lock()
            .map_err(|_| napi::Error::from_reason("stream lock failed".to_string()))?
            .insert(id, session);
    }

    Ok(StreamReadResult { chunks, is_complete })
}

#[napi]
pub fn stream_write(id: String, data: String, close: Option<bool>) -> Result<bool> {
    let mut store = sessions()
        .lock()
        .map_err(|_| napi::Error::from_reason("stream lock failed".to_string()))?;
    let Some(session) = store.get_mut(&id) else {
        return Ok(false);
    };
    if let Some(stdin) = session.stdin_pipe.as_mut() {
        if !data.is_empty() {
            stdin
                .write_all(data.as_bytes())
                .map_err(|err| napi::Error::from_reason(err.to_string()))?;
            stdin
                .flush()
                .map_err(|err| napi::Error::from_reason(err.to_string()))?;
        }
        if close.unwrap_or(false) {
            session.stdin_pipe = None;
        }
        return Ok(true);
    }
    Ok(false)
}

#[napi]
pub fn stream_kill(id: String) -> Result<bool> {
    let mut store = sessions()
        .lock()
        .map_err(|_| napi::Error::from_reason("stream lock failed".to_string()))?;
    let Some(session) = store.get_mut(&id) else {
        return Ok(false);
    };
    if session.reason.is_none() {
        session.reason = Some("killed".to_string());
    }
    let _ = session.child.kill();
    Ok(true)
}

#[napi]
pub fn stream_command(config: StreamConfig) -> Result<StreamChunk> {
    let session = stream_start(StreamStartConfig {
        command: config.command,
        args: config.args,
        cwd: config.cwd,
        env: config.env,
        timeout_ms: config.timeout_ms,
        chunk_size: None,
        stdin_mode: config.stdin_mode,
        use_pty: config.use_pty,
    })?;

    let mut data = String::new();
    let mut stream_type = "stdout".to_string();
    let mut exit_code = None;
    let mut complete_reason = None;

    loop {
        let read = stream_read(session.id.clone(), Some(128), Some(100))?;
        for item in read.chunks {
            if !item.data.is_empty() {
                data.push_str(&item.data);
                stream_type = item.stream_type;
            }
            if item.is_complete {
                exit_code = item.exit_code;
                complete_reason = item.complete_reason;
            }
        }
        if read.is_complete {
            break;
        }
    }

    Ok(StreamChunk {
        data,
        stream_type,
        is_complete: true,
        exit_code,
        complete_reason,
        sequence: None,
        timestamp_ms: None,
    })
}

fn build_shell_command(command: &str, args: &[String]) -> String {
    let mut out = vec![shell_escape(command)];
    out.extend(args.iter().map(|item| shell_escape(item)));
    out.join(" ")
}

fn stdin_from_mode(mode: Option<&str>) -> Stdio {
    match mode {
        Some("piped") => Stdio::piped(),
        _ => Stdio::null(),
    }
}

fn pty_ready() -> bool {
    *PTY_READY.get_or_init(|| {
        let status = Command::new("script")
            .arg("-q")
            .arg("/dev/null")
            .arg("-c")
            .arg("true")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        status.map(|item| item.success()).unwrap_or(false)
    })
}

fn spawn_direct(config: &StreamStartConfig) -> Result<Child> {
    let mut cmd = Command::new(&config.command);
    cmd.args(&config.args)
        .current_dir(&config.cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(stdin_from_mode(config.stdin_mode.as_deref()));
    for (k, v) in &config.env {
        cmd.env(k, v);
    }
    cmd.spawn()
        .map_err(|err| napi::Error::from_reason(err.to_string()))
}

fn shell_escape(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn try_wait_process(child: &mut Child) -> Result<Option<i32>> {
    child
        .try_wait()
        .map_err(|err| napi::Error::from_reason(err.to_string()))
        .map(|item| item.map(|status| status.code().unwrap_or(0)))
}
