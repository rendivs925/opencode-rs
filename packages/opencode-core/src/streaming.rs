use napi::Result;
use std::collections::HashMap;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Clone)]
struct Chunk {
    data: String,
    stream_type: String,
}

struct Session {
    child: Child,
    rx: mpsc::Receiver<Chunk>,
    complete_sent: bool,
    started: Instant,
    timeout_ms: Option<u32>,
    reason: Option<String>,
}

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
pub struct StreamStartConfig {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: HashMap<String, String>,
    pub timeout_ms: Option<u32>,
    pub chunk_size: Option<u32>,
}

#[napi(object)]
#[derive(Clone)]
pub struct StreamChunk {
    pub data: String,
    pub stream_type: String,
    pub is_complete: bool,
    pub exit_code: Option<i32>,
    pub complete_reason: Option<String>,
}

#[napi(object)]
#[derive(Clone)]
pub struct StreamSession {
    pub id: String,
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
) {
    let stream_type = stream_type.to_string();
    thread::spawn(move || {
        let mut buf = vec![0u8; chunk_size.max(1024)];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    let _ = tx.send(Chunk {
                        data,
                        stream_type: stream_type.clone(),
                    });
                }
                Err(_) => break,
            }
        }
    });
}

#[napi]
pub fn stream_start(config: StreamStartConfig) -> Result<StreamSession> {
    let mut cmd = Command::new(&config.command);
    cmd.args(&config.args)
        .current_dir(&config.cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    for (k, v) in &config.env {
        cmd.env(k, v);
    }

    let mut child = cmd
        .spawn()
        .map_err(|err| napi::Error::from_reason(err.to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| napi::Error::from_reason("missing stdout".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| napi::Error::from_reason("missing stderr".to_string()))?;

    let chunk_size = config.chunk_size.unwrap_or(64 * 1024) as usize;
    let (tx, rx) = mpsc::channel::<Chunk>();
    spawn_reader(stdout, tx.clone(), "stdout", chunk_size);
    spawn_reader(stderr, tx, "stderr", chunk_size);

    let id = Uuid::new_v4().to_string();
    sessions()
        .lock()
        .map_err(|_| napi::Error::from_reason("stream lock failed".to_string()))?
        .insert(
            id.clone(),
            Session {
                child,
                rx,
                complete_sent: false,
                started: Instant::now(),
                timeout_ms: config.timeout_ms,
                reason: None,
            },
        );

    Ok(StreamSession { id })
}

#[napi]
pub fn stream_read(id: String, max_chunks: Option<u32>, wait_ms: Option<u32>) -> Result<StreamReadResult> {
    let mut store = sessions()
        .lock()
        .map_err(|_| napi::Error::from_reason("stream lock failed".to_string()))?;
    let Some(session) = store.get_mut(&id) else {
        return Ok(StreamReadResult {
            chunks: vec![],
            is_complete: true,
        });
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
            }),
            Err(mpsc::TryRecvError::Empty) => break,
            Err(mpsc::TryRecvError::Disconnected) => break,
        }
    }

    if !session.complete_sent {
        if let Some(status) = session
            .child
            .try_wait()
            .map_err(|err| napi::Error::from_reason(err.to_string()))?
        {
            session.complete_sent = true;
            let reason = session.reason.clone().unwrap_or_else(|| "exit".to_string());
            chunks.push(StreamChunk {
                data: String::new(),
                stream_type: "stdout".to_string(),
                is_complete: true,
                exit_code: status.code(),
                complete_reason: Some(reason),
            });
        }
    }

    let is_complete = session.complete_sent;
    if is_complete {
        store.remove(&id);
    }

    Ok(StreamReadResult {
        chunks,
        is_complete,
    })
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
    })
}
