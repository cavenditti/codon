use serde::Serialize;
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

const WRITE_DEBOUNCE: Duration = Duration::from_millis(300);

enum Command<T> {
    Schedule(T),
    Flush(mpsc::Sender<()>),
    Shutdown,
}

struct WriterInner<T: Send + 'static> {
    sender: mpsc::Sender<Command<T>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl<T: Send + 'static> Drop for WriterInner<T> {
    fn drop(&mut self) {
        let _ = self.sender.send(Command::Shutdown);
        let worker = match self.worker.get_mut() {
            Ok(worker) => worker.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };
        if let Some(worker) = worker {
            let _ = worker.join();
        }
    }
}

pub(crate) struct DebouncedWriter<T: Send + 'static> {
    inner: Arc<WriterInner<T>>,
}

impl<T: Send + 'static> Clone for DebouncedWriter<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: Send + 'static> fmt::Debug for DebouncedWriter<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DebouncedWriter").finish_non_exhaustive()
    }
}

impl<T: Send + 'static> DebouncedWriter<T> {
    pub(crate) fn toml(path: PathBuf, label: &'static str) -> Self
    where
        T: Serialize,
    {
        Self::spawn(WRITE_DEBOUNCE, move |value| {
            let serialized = match toml::to_string_pretty(value) {
                Ok(serialized) => serialized,
                Err(err) => {
                    log::warn!("file-manager: serialising {label} failed: {err}");
                    return;
                }
            };
            if let Some(parent) = path.parent()
                && let Err(err) = std::fs::create_dir_all(parent)
            {
                log::warn!(
                    "file-manager: could not create {} ({err})",
                    parent.display()
                );
                return;
            }
            if let Err(err) = std::fs::write(&path, serialized) {
                log::warn!(
                    "file-manager: could not persist {label} to {} ({err})",
                    path.display()
                );
            }
        })
    }

    fn spawn(debounce: Duration, persist: impl Fn(&T) + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::channel();
        let worker = match std::thread::Builder::new()
            .name("fm-config-writer".into())
            .spawn(move || run_worker(receiver, debounce, persist))
        {
            Ok(worker) => Some(worker),
            Err(err) => {
                log::warn!("file-manager: could not start config writer ({err})");
                None
            }
        };
        Self {
            inner: Arc::new(WriterInner {
                sender,
                worker: Mutex::new(worker),
            }),
        }
    }

    pub(crate) fn schedule(&self, value: T) {
        let _ = self.inner.sender.send(Command::Schedule(value));
    }

    /// Drain the last scheduled snapshot. This is intentionally blocking:
    /// normal setters only call `schedule`; `flush` is reserved for the app
    /// quit path and deterministic tests.
    pub(crate) fn flush(&self) {
        let (done_tx, done_rx) = mpsc::channel();
        if self.inner.sender.send(Command::Flush(done_tx)).is_ok() {
            let _ = done_rx.recv();
        }
    }
}

fn run_worker<T: Send + 'static>(
    receiver: mpsc::Receiver<Command<T>>,
    debounce: Duration,
    persist: impl Fn(&T),
) {
    let mut pending = None;
    loop {
        let command = if pending.is_some() {
            match receiver.recv_timeout(debounce) {
                Ok(command) => command,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    persist_pending(&mut pending, &persist);
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    persist_pending(&mut pending, &persist);
                    return;
                }
            }
        } else {
            match receiver.recv() {
                Ok(command) => command,
                Err(_) => return,
            }
        };

        match command {
            Command::Schedule(value) => pending = Some(value),
            Command::Flush(done) => {
                persist_pending(&mut pending, &persist);
                let _ = done.send(());
            }
            Command::Shutdown => {
                persist_pending(&mut pending, &persist);
                return;
            }
        }
    }
}

fn persist_pending<T>(pending: &mut Option<T>, persist: &impl Fn(&T)) {
    if let Some(value) = pending.take() {
        persist(&value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn repeated_schedules_coalesce_to_the_latest_snapshot() {
        let writes = Arc::new(AtomicUsize::new(0));
        let last_value = Arc::new(AtomicUsize::new(0));
        let writer = DebouncedWriter::spawn(Duration::from_secs(60), {
            let writes = writes.clone();
            let last_value = last_value.clone();
            move |value: &usize| {
                writes.fetch_add(1, Ordering::SeqCst);
                last_value.store(*value, Ordering::SeqCst);
            }
        });

        for value in 0..100 {
            writer.schedule(value);
        }
        writer.flush();

        assert_eq!(writes.load(Ordering::SeqCst), 1);
        assert_eq!(last_value.load(Ordering::SeqCst), 99);
    }

    #[test]
    fn dropping_last_handle_flushes_pending_snapshot() {
        let writes = Arc::new(AtomicUsize::new(0));
        let last_value = Arc::new(AtomicUsize::new(0));
        let writer = DebouncedWriter::spawn(Duration::from_secs(60), {
            let writes = writes.clone();
            let last_value = last_value.clone();
            move |value: &usize| {
                writes.fetch_add(1, Ordering::SeqCst);
                last_value.store(*value, Ordering::SeqCst);
            }
        });

        writer.schedule(42);
        drop(writer);

        assert_eq!(writes.load(Ordering::SeqCst), 1);
        assert_eq!(last_value.load(Ordering::SeqCst), 42);
    }
}
