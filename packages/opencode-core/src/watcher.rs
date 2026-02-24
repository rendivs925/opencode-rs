use napi::Result;

#[napi]
pub struct FileWatcher {
    id: String,
    path: String,
}

#[napi]
pub struct FileEvent {
    pub path: String,
    pub kind: String,
}

#[napi]
impl FileWatcher {
    #[napi]
    pub fn new() -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        FileWatcher {
            id,
            path: String::new(),
        }
    }

    #[napi]
    pub fn watch(&mut self, path: String) -> Result<()> {
        self.path = path;
        Ok(())
    }

    #[napi]
    pub fn unwatch(&mut self) -> Result<()> {
        self.path = String::new();
        Ok(())
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
