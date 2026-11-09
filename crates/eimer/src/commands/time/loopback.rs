//! Receives the OIDC redirect on a local port (`http://localhost:<port>/<path>`).

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use reqwest::Url;

const TIMEOUT: Duration = Duration::from_secs(300);
const PAGE: &str = "<!doctype html><meta charset=utf-8><title>eimer time</title>\
<body style=\"font-family:system-ui;margin:4rem\"><h1>Logged in</h1>\
<p>You can close this tab and return to the terminal.</p>";

/// A redirect URI eimer can serve itself.
pub fn is_loopback(redirect_uri: &str) -> bool {
    Url::parse(redirect_uri).is_ok_and(|url| {
        url.scheme() == "http"
            && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
            && url.port().is_some()
    })
}

/// Bound listeners for the redirect. Binds both IPv4 and IPv6 loopback, since
/// browsers resolve `localhost` to either.
pub struct Callback {
    listeners: Vec<TcpListener>,
    path: String,
}

impl Callback {
    pub fn bind(redirect_uri: &str) -> Result<Self> {
        let url = Url::parse(redirect_uri).context("Failed to parse the redirect URI")?;
        let port = url
            .port()
            .ok_or_else(|| anyhow!("Redirect URI needs a port"))?;
        let addrs: [SocketAddr; 2] = [
            ([127, 0, 0, 1], port).into(),
            ([0, 0, 0, 0, 0, 0, 0, 1], port).into(),
        ];
        let listeners: Vec<_> = addrs
            .iter()
            .filter_map(|a| TcpListener::bind(a).ok())
            .collect();
        if listeners.is_empty() {
            bail!("Failed to listen on port {port}; is another login running?");
        }
        Ok(Self {
            listeners,
            path: url.path().to_string(),
        })
    }

    /// Waits for the browser and returns the redirect's full URL (with query).
    pub fn wait(self) -> Result<Url> {
        let (sender, receiver) = mpsc::channel();
        for listener in self.listeners {
            let (sender, path) = (sender.clone(), self.path.clone());
            std::thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    if let Some(url) = handle(stream, &path) {
                        let _ = sender.send(url);
                        return;
                    }
                }
            });
        }
        receiver
            .recv_timeout(TIMEOUT)
            .map_err(|_| anyhow!("Timed out waiting for the login redirect"))
    }
}

/// Answers one request; returns the URL when it hit the callback path.
fn handle(mut stream: TcpStream, path: &str) -> Option<Url> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut request_line = String::new();
    BufReader::new(&stream).read_line(&mut request_line).ok()?;
    let target = request_line.split_whitespace().nth(1)?;
    let url = Url::parse(&format!("http://localhost{target}")).ok()?;
    if url.path() != path {
        let _ = stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        return None;
    }
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{PAGE}",
        PAGE.len()
    );
    let _ = stream.write_all(response.as_bytes());
    Some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_loopback_redirects() {
        assert!(is_loopback("http://localhost:1995/callback"));
        assert!(is_loopback("http://127.0.0.1:1995/callback"));
        assert!(!is_loopback("http://localhost/callback"));
        assert!(!is_loopback(
            "https://raycast.com/redirect?packageName=Extension"
        ));
    }

    #[test]
    fn receives_the_callback_and_ignores_other_paths() {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let callback = Callback::bind(&format!("http://localhost:{port}/callback")).unwrap();
        let get = move |target: &str| {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(stream, "GET {target} HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
            let mut status = String::new();
            BufReader::new(stream).read_line(&mut status).unwrap();
            status
        };
        let client =
            std::thread::spawn(move || (get("/favicon.ico"), get("/callback?code=abc&state=s1")));
        let url = callback.wait().unwrap();
        let (ignored, served) = client.join().unwrap();
        assert!(ignored.contains("404"));
        assert!(served.contains("200"));
        assert_eq!(url.query(), Some("code=abc&state=s1"));
    }
}
