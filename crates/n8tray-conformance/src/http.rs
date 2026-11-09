//! A minimal HTTP/1.1 client for loopback probes. The probe must send exact
//! headers (a forged `Host`, an `Origin`), so it speaks the protocol itself
//! instead of through a client library that would normalize them.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

/// A response: status code, lowercase header names, body.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Response {
    /// The first header named `name` (lowercase).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// A request to `127.0.0.1:<port>`.
#[derive(Debug, Clone)]
pub struct Request<'a> {
    pub method: &'a str,
    pub port: u16,
    pub path: &'a str,
    /// Extra headers. A `Host` here replaces the default `127.0.0.1:<port>`.
    pub headers: Vec<(&'a str, String)>,
    pub body: Option<&'a str>,
    pub timeout: Duration,
}

impl<'a> Request<'a> {
    pub fn new(method: &'a str, port: u16, path: &'a str, timeout: Duration) -> Self {
        Request {
            method,
            port,
            path,
            headers: Vec::new(),
            body: None,
            timeout,
        }
    }

    pub fn header(mut self, name: &'a str, value: impl Into<String>) -> Self {
        self.headers.push((name, value.into()));
        self
    }

    pub fn json(mut self, body: &'a str) -> Self {
        self.body = Some(body);
        self.headers
            .push(("Content-Type", "application/json".into()));
        self
    }

    /// Sends the request and reads the whole response.
    pub fn send(&self) -> io::Result<Response> {
        let address = SocketAddr::from(([127, 0, 0, 1], self.port));
        let mut stream = TcpStream::connect_timeout(&address, self.timeout)?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;

        let mut head = format!("{} {} HTTP/1.1\r\n", self.method, self.path);
        if !self
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("host"))
        {
            head.push_str(&format!("Host: 127.0.0.1:{}\r\n", self.port));
        }
        for (name, value) in &self.headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        let body = self.body.unwrap_or("");
        if self.body.is_some() || self.method == "POST" {
            head.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }
        head.push_str("Connection: close\r\n\r\n");
        stream.write_all(head.as_bytes())?;
        stream.write_all(body.as_bytes())?;
        stream.flush()?;
        read_response(BufReader::new(stream))
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

fn read_response(mut reader: impl BufRead) -> io::Result<Response> {
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let status = line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| invalid("not an HTTP response"))?;
    let mut headers = Vec::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    let find = |name: &str| {
        headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    };
    let mut body = Vec::new();
    if find("transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked")) {
        loop {
            line.clear();
            reader.read_line(&mut line)?;
            let size = usize::from_str_radix(line.trim().split(';').next().unwrap_or(""), 16)
                .map_err(|_| invalid("bad chunk size"))?;
            if size == 0 {
                break;
            }
            let mut chunk = vec![0; size];
            reader.read_exact(&mut chunk)?;
            body.extend_from_slice(&chunk);
            line.clear();
            reader.read_line(&mut line)?;
        }
    } else if let Some(length) = find("content-length").and_then(|v| v.parse::<usize>().ok()) {
        body.resize(length, 0);
        reader.read_exact(&mut body)?;
    } else {
        reader.read_to_end(&mut body)?;
    }
    Ok(Response {
        status,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_content_length_and_chunked_bodies() {
        let plain = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nX-A: b\r\n\r\nhi";
        let response = read_response(plain.as_bytes()).unwrap();
        assert_eq!(
            (
                response.status,
                response.body.as_str(),
                response.header("x-a")
            ),
            (200, "hi", Some("b"))
        );
        let chunked = "HTTP/1.1 401 Unauthorized\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n";
        let response = read_response(chunked.as_bytes()).unwrap();
        assert_eq!((response.status, response.body.as_str()), (401, "abcde"));
    }
}
