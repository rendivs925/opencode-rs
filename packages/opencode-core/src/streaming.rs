use napi::Result;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
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

enum ProcessHandle {
    Pipe(Child),
    Pty(Box<dyn portable_pty::Child + Send>),
}

struct Session {
    process: ProcessHandle,
    stdin_pipe: Option<ChildStdin>,
    pty_writer: Option<Box<dyn Write + Send>>,
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
    let (pid, process, stdin_pipe, pty_writer, sequence) = if use_pty {
        match spawn_pty(&config, tx.clone(), chunk_size) {
            Ok(item) => item,
            Err(_) => spawn_direct(&config, tx, chunk_size)?,
        }
    } else {
        spawn_direct(&config, tx, chunk_size)?
    };
    let session = Session {
        process,
        stdin_pipe,
        pty_writer,
        rx,
        complete_sent: false,
        started: Instant::now(),
        timeout_ms: config.timeout_ms,
        reason: None,
        sequence,
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
                let _ = kill_process(&mut session.process);
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
        if let Some(exit) = try_wait_process(&mut session.process)? {
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
    if let Some(writer) = session.pty_writer.as_mut() {
        if !data.is_empty() {
            writer
                .write_all(data.as_bytes())
                .map_err(|err| napi::Error::from_reason(err.to_string()))?;
            writer
                .flush()
                .map_err(|err| napi::Error::from_reason(err.to_string()))?;
        }
        if close.unwrap_or(false) {
            session.pty_writer = None;
        }
        return Ok(true);
    }
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
    let _ = kill_process(&mut session.process);
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

fn stdin_from_mode(mode: Option<&str>) -> Stdio {
    match mode {
        Some("piped") => Stdio::piped(),
        _ => Stdio::null(),
    }
}

fn spawn_direct(
    config: &StreamStartConfig,
    tx: mpsc::Sender<Chunk>,
    chunk_size: usize,
) -> Result<(u32, ProcessHandle, Option<ChildStdin>, Option<Box<dyn Write + Send>>, u64)> {
    let mut cmd = Command::new(&config.command);
    cmd.args(&config.args)
        .current_dir(&config.cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(stdin_from_mode(config.stdin_mode.as_deref()));
    for (k, v) in &config.env {
        cmd.env(k, v);
    }
    let mut child = cmd
        .spawn()
        .map_err(|err| napi::Error::from_reason(err.to_string()))
        ?;
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
    Ok((pid, ProcessHandle::Pipe(child), stdin_pipe, None, 2))
}

fn spawn_pty(
    config: &StreamStartConfig,
    tx: mpsc::Sender<Chunk>,
    chunk_size: usize,
) -> Result<(u32, ProcessHandle, Option<ChildStdin>, Option<Box<dyn Write + Send>>, u64)> {
    let system = native_pty_system();
    let pair = system
        .openpty(PtySize {
            rows: 30,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|err| napi::Error::from_reason(err.to_string()))?;
    let mut cmd = CommandBuilder::new(&config.command);
    cmd.cwd(&config.cwd);
    for arg in &config.args {
        cmd.arg(arg);
    }
    for (k, v) in &config.env {
        cmd.env(k, v);
    }
    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|err| napi::Error::from_reason(err.to_string()))?;
    let pid = child.process_id().unwrap_or(0);
    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|err| napi::Error::from_reason(err.to_string()))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|err| napi::Error::from_reason(err.to_string()))?;
    spawn_reader(reader, tx, "stdout", chunk_size, 0);
    Ok((pid, ProcessHandle::Pty(child), None, Some(writer), 1))
}

fn kill_process(process: &mut ProcessHandle) -> Result<()> {
    match process {
        ProcessHandle::Pipe(child) => child
            .kill()
            .map_err(|err| napi::Error::from_reason(err.to_string())),
        ProcessHandle::Pty(child) => child
            .kill()
            .map_err(|err| napi::Error::from_reason(err.to_string())),
    }
}

fn try_wait_process(process: &mut ProcessHandle) -> Result<Option<i32>> {
    match process {
        ProcessHandle::Pipe(child) => child
            .try_wait()
            .map_err(|err| napi::Error::from_reason(err.to_string()))
            .map(|item| item.map(|status| status.code().unwrap_or(0))),
        ProcessHandle::Pty(child) => child
            .try_wait()
            .map_err(|err| napi::Error::from_reason(err.to_string()))
            .map(|item| item.map(|status| status.exit_code() as i32)),
    }
}
