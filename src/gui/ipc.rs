use super::{Action, Event};
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

/// 第二個實例只送喚醒訊息；拒絕連線的舊 socket 才刪除重建。
/// 目錄或 socket 的權限、連線與綁定錯誤往上傳，不啟動第二份背景工作。
pub fn acquire(dir: &Path, events: Sender<Event>) -> io::Result<Option<Instance>> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    let path = dir.join("gui.sock");
    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(err) if err.kind() == io::ErrorKind::AddrInUse => match UnixStream::connect(&path) {
            Ok(mut stream) => {
                stream.write_all(b"show")?;
                return Ok(None);
            }
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
                ) =>
            {
                match fs::remove_file(&path) {
                    Ok(()) => {}
                    Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                    Err(err) => return Err(err),
                }
                UnixListener::bind(&path)?
            }
            Err(err) => return Err(err),
        },
        Err(err) => return Err(err),
    };
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let (stop, rx): (Sender<()>, Receiver<()>) = mpsc::channel();
    thread::spawn(move || {
        while rx.try_recv().is_err() {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                    let mut bytes = [0; 4];
                    if stream.read_exact(&mut bytes).is_ok() && &bytes == b"show" {
                        let _ = events.send(Event::Action(Action::Show));
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                }
                Err(_) => break,
            }
        }
    });
    Ok(Some(Instance { path, stop }))
}
pub struct Instance {
    path: PathBuf,
    stop: Sender<()>,
}
impl Drop for Instance {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn second_instance_notifies_first_and_does_not_bind_again() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let _instance = acquire(dir.path(), tx.clone()).unwrap().unwrap();
        assert!(acquire(dir.path(), tx).unwrap().is_none());
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            Event::Action(Action::Show)
        ));
    }
    #[test]
    fn stale_socket_is_replaced_by_new_instance() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _) = mpsc::channel();
        drop(UnixListener::bind(dir.path().join("gui.sock")).unwrap());
        assert!(acquire(dir.path(), tx).unwrap().is_some());
    }
}
