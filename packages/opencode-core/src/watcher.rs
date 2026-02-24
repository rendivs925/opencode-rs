use crate::error::CoreError;
use ignore::gitignore::GitignoreBuilder;
use napi::Result;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;
use uuid::Uuid;

#[napi]
pub struct FileWatcher {
    id: String,
    path: String,
    watcher: Option<RecommendedWatcher>,
    rx: Option<Receiver<std::result::Result<Event, notify::Error>>>,
}

#[napi]
pub struct FileEvent {
    pub path: String,
    pub kind: String,
}

#[napi]
impl FileWatcher {
    #[napi(constructor)]
    pub fn new() -> Self {
        let id = Uuid::new_v4().to_string();
        FileWatcher {
            id,
            path: String::new(),
            watcher: None,
            rx: None,
        }
    }

    #[napi]
    pub fn watch(&mut self, path: String) -> Result<()> {
        self.path = path.clone();

        let (tx, rx) = channel();
        let mut watcher: RecommendedWatcher = Watcher::new(
            tx,
            Config::default().with_poll_interval(Duration::from_millis(500)),
        )
        .map_err(CoreError::from)?;
        watcher
            .watch(Path::new(&path), RecursiveMode::Recursive)
            .map_err(CoreError::from)?;

        self.watcher = Some(watcher);
        self.rx = Some(rx);

        Ok(())
    }

    #[napi]
    pub fn unwatch(&mut self) -> Result<()> {
        self.watcher.take();
        self.rx.take();
        self.path = String::new();
        Ok(())
    }

    #[napi]
    pub fn next_event(&mut self) -> Result<Option<FileEvent>> {
        if self.rx.is_none() {
            return Err(napi::Error::from_reason("Watcher not initialized"));
        }

        let rx = self.rx.as_ref().unwrap();

        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) => {
                if let Some(path) = event.paths.first() {
                    let kind = match event.kind {
                        notify::EventKind::Create(_) => "create",
                        notify::EventKind::Modify(_) => "write",
                        notify::EventKind::Remove(_) => "remove",
                        _ => return Ok(None),
                    };
                    return Ok(Some(FileEvent {
                        path: path.to_string_lossy().to_string(),
                        kind: kind.to_string(),
                    }));
                }
                Ok(None)
            }
            Ok(Err(_)) => Ok(None),
            Err(_) => Ok(None),
        }
    }

    #[napi]
    pub fn next_events(
        &mut self,
        limit: Option<i32>,
        ignore_patterns: Option<Vec<String>>,
    ) -> Result<Vec<FileEvent>> {
        if self.rx.is_none() {
            return Err(napi::Error::from_reason("Watcher not initialized"));
        }
        let max = limit.unwrap_or(128).max(1) as usize;
        let rx = self.rx.as_ref().unwrap();
        let mut out = Vec::new();

        while out.len() < max {
            let item = if out.is_empty() {
                rx.recv_timeout(Duration::from_millis(100))
            } else {
                match rx.try_recv() {
                    Ok(evt) => Ok(evt),
                    Err(_) => break,
                }
            };
            let event = match item {
                Ok(Ok(event)) => event,
                Ok(Err(_)) => continue,
                Err(_) => break,
            };
            out.extend(filter_events(
                &event,
                &self.path,
                ignore_patterns.as_ref(),
            ));
        }

        Ok(coalesce_events(out, max))
    }
}

impl Default for FileWatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
pub fn watch_path(path: String) -> Result<FileWatcher> {
    let mut watcher = FileWatcher::new();
    watcher.watch(path)?;
    Ok(watcher)
}

fn filter_events(event: &Event, root: &str, patterns: Option<&Vec<String>>) -> Vec<FileEvent> {
    let kind = match event.kind {
        notify::EventKind::Create(_) => "add",
        notify::EventKind::Modify(_) => "change",
        notify::EventKind::Remove(_) => "unlink",
        _ => return Vec::new(),
    };

    event
        .paths
        .iter()
        .filter_map(|item| {
            let rel = item
                .strip_prefix(Path::new(root))
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|| item.to_string_lossy().replace('\\', "/"));
            if rel.is_empty() {
                return None;
            }
            if should_ignore(&rel, patterns) {
                return None;
            }
            Some(FileEvent {
                path: item.to_string_lossy().to_string(),
                kind: kind.to_string(),
            })
        })
        .collect()
}

fn should_ignore(path: &str, patterns: Option<&Vec<String>>) -> bool {
    let Some(patterns) = patterns else {
        return false;
    };
    if patterns.is_empty() {
        return false;
    }

    let mut builder = GitignoreBuilder::new("");
    for pattern in patterns {
        if builder.add_line(None, pattern).is_err() {
            continue;
        }
    }
    if let Ok(matcher) = builder.build() {
        return matcher.matched(Path::new(path), false).is_ignore();
    }
    false
}

fn coalesce_events(events: Vec<FileEvent>, limit: usize) -> Vec<FileEvent> {
    let mut order = Vec::new();
    let mut map = HashMap::<String, String>::new();

    for item in events {
        if !map.contains_key(&item.path) {
            order.push(item.path.clone());
            map.insert(item.path, item.kind);
            continue;
        }
        let prev = map.get(&item.path).cloned().unwrap_or_default();
        let next = merge_kind(&prev, &item.kind);
        if next.is_empty() {
            map.remove(&item.path);
            continue;
        }
        map.insert(item.path, next);
    }

    let mut out = Vec::new();
    for path in order {
        if out.len() >= limit {
            break;
        }
        let Some(kind) = map.get(&path) else {
            continue;
        };
        out.push(FileEvent {
            path,
            kind: kind.to_string(),
        });
    }
    out
}

fn merge_kind(prev: &str, next: &str) -> String {
    if prev == "add" && next == "change" {
        return "add".to_string();
    }
    if prev == "add" && next == "unlink" {
        return String::new();
    }
    if prev == "change" && next == "unlink" {
        return "unlink".to_string();
    }
    if prev == "unlink" && next == "add" {
        return "change".to_string();
    }
    if prev == "unlink" && next == "change" {
        return "change".to_string();
    }
    next.to_string()
}
