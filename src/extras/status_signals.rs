#[cfg(unix)]
use std::io::Write;
#[cfg(unix)]
use std::os::unix::net::UnixStream;

#[derive(Clone)]
pub struct StatusSignals {
    path: String,
}

impl StatusSignals {
    #[allow(dead_code)]
    pub fn new(path: String) -> Self {
        Self { path }
    }

    #[cfg(unix)]
    pub fn send_start(&self) {
        let _ = (|| -> std::io::Result<()> {
            let mut stream = UnixStream::connect(&self.path)?;
            stream.write_all(b"start\n")?;
            Ok(())
        })();
    }

    #[cfg(not(unix))]
    pub fn send_start(&self) {}

    #[cfg(unix)]
    pub fn send_stop(&self) {
        let _ = (|| -> std::io::Result<()> {
            let mut stream = UnixStream::connect(&self.path)?;
            stream.write_all(b"stop\n")?;
            Ok(())
        })();
    }

    #[cfg(not(unix))]
    pub fn send_stop(&self) {}

    #[cfg(unix)]
    pub fn send_git_conflict(&self) {
        let _ = (|| -> std::io::Result<()> {
            let mut stream = UnixStream::connect(&self.path)?;
            stream.write_all(b"git-conflict\n")?;
            Ok(())
        })();
    }

    #[cfg(not(unix))]
    pub fn send_git_conflict(&self) {}
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::Read;
    use std::os::unix::net::UnixListener;

    fn temp_socket_path(name: &str) -> (std::path::PathBuf, UnixListener) {
        let dir =
            std::env::temp_dir().join(format!("zs_status_test_{}_{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let socket_path = dir.join("status.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();
        (socket_path, listener)
    }

    fn cleanup(path: &std::path::Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn send_start_writes_expected_message() {
        let (socket_path, listener) = temp_socket_path("start");
        let ss = StatusSignals::new(socket_path.to_string_lossy().to_string());
        ss.send_start();

        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = String::new();
        stream.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "start\n");
        cleanup(&socket_path);
    }

    #[test]
    fn send_stop_writes_expected_message() {
        let (socket_path, listener) = temp_socket_path("stop");
        let ss = StatusSignals::new(socket_path.to_string_lossy().to_string());
        ss.send_stop();

        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = String::new();
        stream.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "stop\n");
        cleanup(&socket_path);
    }

    #[test]
    fn nonexistent_socket_does_not_panic() {
        let ss = StatusSignals::new("/tmp/definitely_nonexistent_status_socket_12345".to_string());
        ss.send_start();
        ss.send_stop();
        ss.send_git_conflict();
    }

    #[test]
    fn send_git_conflict_writes_expected_message() {
        let (socket_path, listener) = temp_socket_path("conflict");
        let ss = StatusSignals::new(socket_path.to_string_lossy().to_string());
        ss.send_git_conflict();

        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = String::new();
        stream.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "git-conflict\n");
        cleanup(&socket_path);
    }
}
