//! Minimal HTTP server for download tests: serves one byte blob with
//! optional Range support on an ephemeral port until dropped.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct BlobServer {
    pub url: String,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl BlobServer {
    pub fn serve(blob: Vec<u8>, honor_range: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let handle = std::thread::spawn(move || {
            while !stop2.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        let mut reader = BufReader::new(stream.try_clone().unwrap());
                        let mut range: Option<u64> = None;
                        let mut line = String::new();
                        loop {
                            line.clear();
                            if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
                                break;
                            }
                            if let Some(v) = line.to_lowercase().strip_prefix("range: bytes=") {
                                range = v.trim().trim_end_matches('-').parse().ok();
                            }
                        }
                        let (status, body) = match range {
                            Some(from) if honor_range && (from as usize) < blob.len() => {
                                ("206 Partial Content", &blob[from as usize..])
                            }
                            _ => ("200 OK", &blob[..]),
                        };
                        let _ = write!(
                            stream,
                            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(body);
                    }
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(5)),
                }
            }
        });
        Self { url: format!("http://{addr}/blob"), stop, handle: Some(handle) }
    }
}

impl Drop for BlobServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
