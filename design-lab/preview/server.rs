use std::{
    io::{self, BufReader, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};

const MAX_BODY_BYTES: usize = 1024 * 1024;
const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_CONNECTIONS: usize = 8;

pub(super) struct PreviewServer {
    address: SocketAddr,
    stop: Sender<()>,
    stopping: Arc<AtomicBool>,
    worker: JoinHandle<Result<()>>,
}

struct Request {
    method: String,
    target: String,
    body: Vec<u8>,
}

enum ConnectionState {
    Completed,
    Closed,
}

impl PreviewServer {
    pub(super) fn start() -> Result<Self> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .context("启动预览 HTTP 服务失败")?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let (stop, receiver) = mpsc::channel();
        let stopping = Arc::new(AtomicBool::new(false));
        let signal = stopping.clone();
        let worker = thread::Builder::new()
            .name("postui-preview-http".to_string())
            .spawn(move || serve(listener, receiver, signal))
            .context("启动预览 HTTP 线程失败")?;
        Ok(Self {
            address,
            stop,
            stopping,
            worker,
        })
    }

    pub(super) fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }

    pub(super) fn stop(self) -> Result<()> {
        self.stopping.store(true, Ordering::Release);
        let signal = self.stop.send(());
        self.worker
            .join()
            .map_err(|_| anyhow!("预览 HTTP 线程异常结束"))??;
        signal.context("预览 HTTP 服务已断开")
    }
}

fn serve(listener: TcpListener, stop: Receiver<()>, stopping: Arc<AtomicBool>) -> Result<()> {
    let mut connections: Vec<JoinHandle<io::Result<ConnectionState>>> = Vec::new();
    let result = (|| -> Result<()> {
        loop {
            for index in (0..connections.len()).rev() {
                if connections[index].is_finished() {
                    finish_connection(connections.swap_remove(index))?;
                }
            }
            if connections.len() < MAX_CONNECTIONS {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let signal = stopping.clone();
                        connections.push(
                            thread::Builder::new()
                                .name("postui-preview-request".to_string())
                                .spawn(move || handle_connection(stream, &signal))?,
                        );
                        continue;
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                    Err(error) => return Err(error).context("接收预览 HTTP 连接失败"),
                }
            }
            match stop.recv_timeout(Duration::from_millis(16)) {
                Ok(()) => return Ok(()),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(anyhow!("预览 HTTP 停止信号已断开"));
                }
            }
        }
    })();
    stopping.store(true, Ordering::Release);
    let mut cleanup = Ok(());
    for connection in connections {
        cleanup = super::finish(cleanup, finish_connection(connection));
    }
    super::finish(result, cleanup)
}

fn finish_connection(worker: JoinHandle<io::Result<ConnectionState>>) -> Result<()> {
    worker
        .join()
        .map_err(|_| anyhow!("预览 HTTP 请求线程异常结束"))??;
    Ok(())
}

fn handle_connection(mut stream: TcpStream, stopping: &AtomicBool) -> io::Result<ConnectionState> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    match respond(&mut stream, stopping) {
        Ok(()) => Ok(ConnectionState::Completed),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::BrokenPipe
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::UnexpectedEof
                    | io::ErrorKind::TimedOut
                    | io::ErrorKind::WouldBlock
            ) =>
        {
            Ok(ConnectionState::Closed)
        }
        Err(error) => Err(error),
    }
}

fn respond(stream: &mut TcpStream, stopping: &AtomicBool) -> io::Result<()> {
    let request = match read_request(stream) {
        Ok(request) => request,
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            return write_response(
                stream,
                400,
                &json!({"error": error.to_string()}),
                false,
                false,
            );
        }
        Err(error) => return Err(error),
    };
    let path = request.target.split('?').next().unwrap_or(&request.target);
    if path == "/slow" {
        for _ in 0..90 {
            if stopping.load(Ordering::Acquire) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
    let (status, body) = match (request.method.as_str(), path) {
        ("GET" | "HEAD", "/health") => (
            200,
            json!({
                "status": "healthy", "service": "PostUI Studio", "version": "1.0.0",
                "region": "local", "checks": {"api": "ready", "storage": "ready", "queue": "idle"}
            }),
        ),
        ("GET" | "HEAD", "/projects" | "/export") => {
            let names = [
                "Terminal Studio",
                "Atlas",
                "Field Notes",
                "Northstar",
                "Monochrome",
                "Daylight",
            ];
            let projects: Vec<_> = (0..24).map(|index| json!({
                "id": format!("p-{}", 1047 + index),
                "name": format!("{} {}", names[index % names.len()], index / names.len() + 1),
                "owner": (["Lin", "Alex", "Sam"][index % 3]),
                "status": "active", "members": 3 + index, "visibility": "private"
            })).collect();
            (
                200,
                json!({"data": projects, "pagination": {"page": 1, "limit": 24, "total": 24}}),
            )
        }
        ("POST", "/projects") | ("PATCH", "/projects/p-1047") | (_, "/echo") => {
            match serde_json::from_slice::<Value>(&request.body) {
                Ok(received) => (
                    if request.method == "POST" { 201 } else { 200 },
                    json!({
                        "data": {"id": "p-1047", "status": "active", "project": received},
                        "meta": {"request_id": "req-studio-1047", "region": "local", "api_version": "2026-09"}
                    }),
                ),
                Err(_) => (
                    400,
                    json!({"error": {"code": "INVALID_JSON", "message": "请求体不是有效 JSON"}}),
                ),
            }
        }
        (_, "/status/422") => (
            422,
            json!({
                "error": {"code": "VALIDATION_FAILED", "message": "字段校验未通过", "fields": [
                    {"field": "name", "status": "required"},
                    {"field": "visibility", "allowed": ["private", "public"]}
                ]}, "request_id": "req-studio-1048"
            }),
        ),
        (_, "/slow") => (
            200,
            json!({"status": "complete", "delay_ms": 1800, "data": {"jobs": 12, "processed": 12}}),
        ),
        ("POST", "/upload") => (
            201,
            json!({"status": "received", "bytes": request.body.len(), "collection": "studio"}),
        ),
        _ => (404, json!({"error": {"code": "NOT_FOUND", "path": path}})),
    };
    write_response(
        stream,
        status,
        &body,
        path == "/export",
        request.method == "HEAD",
    )
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    body: &Value,
    attachment: bool,
    head: bool,
) -> io::Result<()> {
    let body = serde_json::to_vec(body).map_err(io::Error::other)?;
    let reason = match status {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        404 => "Not Found",
        422 => "Unprocessable Entity",
        _ => "Unknown",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nServer: postui-preview\r\nX-Request-Id: req-studio-1047\r\nCache-Control: no-store\r\nConnection: close\r\n",
        body.len()
    )?;
    if attachment {
        write!(
            stream,
            "Content-Disposition: attachment; filename=studio-projects.json\r\n"
        )?;
    }
    write!(stream, "\r\n")?;
    if !head {
        stream.write_all(&body)?;
    }
    stream.flush()
}

fn read_request(stream: &mut TcpStream) -> io::Result<Request> {
    let mut reader = BufReader::new(stream);
    let mut remaining = MAX_HEADER_BYTES;
    let request_line = read_line(&mut reader, &mut remaining)?;
    let mut fields = request_line.split_whitespace();
    let method = fields
        .next()
        .ok_or_else(|| invalid("请求方法为空"))?
        .to_string();
    let target = fields
        .next()
        .ok_or_else(|| invalid("请求路径为空"))?
        .to_string();
    if !matches!(fields.next(), Some("HTTP/1.0" | "HTTP/1.1")) || fields.next().is_some() {
        return Err(invalid("HTTP 请求行无效"));
    }
    let mut content_length = None;
    let mut chunked = false;
    loop {
        let header = read_line(&mut reader, &mut remaining)?;
        if header.is_empty() {
            break;
        }
        let (name, value) = header
            .split_once(':')
            .ok_or_else(|| invalid("HTTP 请求头无效"))?;
        if name.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return Err(invalid("Content-Length 重复"));
            }
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| invalid("Content-Length 无效"))?,
            );
        }
        if name.eq_ignore_ascii_case("transfer-encoding") {
            if !value.trim().eq_ignore_ascii_case("chunked") {
                return Err(invalid("预览 HTTP 服务支持 chunked 传输编码"));
            }
            chunked = true;
        }
    }
    if chunked && content_length.is_some() {
        return Err(invalid("Content-Length 与 Transfer-Encoding 同时存在"));
    }
    let mut body = Vec::new();
    if chunked {
        loop {
            let size = read_line(&mut reader, &mut remaining)?;
            let size = size.split(';').next().unwrap_or(&size);
            let size =
                usize::from_str_radix(size.trim(), 16).map_err(|_| invalid("分块长度无效"))?;
            if size == 0 {
                while !read_line(&mut reader, &mut remaining)?.is_empty() {}
                break;
            }
            read_body(&mut reader, &mut body, size)?;
            if !read_line(&mut reader, &mut remaining)?.is_empty() {
                return Err(invalid("分块结束标记无效"));
            }
        }
    } else if let Some(length) = content_length {
        read_body(&mut reader, &mut body, length)?;
    }
    Ok(Request {
        method,
        target,
        body,
    })
}

fn read_body(reader: &mut impl Read, body: &mut Vec<u8>, length: usize) -> io::Result<()> {
    if length > MAX_BODY_BYTES.saturating_sub(body.len()) {
        return Err(invalid("预览请求体上限为 1 MiB"));
    }
    let start = body.len();
    body.resize(start + length, 0);
    reader.read_exact(&mut body[start..])
}

fn read_line(reader: &mut impl Read, remaining: &mut usize) -> io::Result<String> {
    let mut line = Vec::new();
    loop {
        if *remaining == 0 {
            return Err(invalid("预览请求头上限为 64 KiB"));
        }
        let mut byte = [0];
        reader.read_exact(&mut byte)?;
        *remaining -= 1;
        line.push(byte[0]);
        if line.ends_with(b"\r\n") {
            line.truncate(line.len() - 2);
            return String::from_utf8(line).map_err(|_| invalid("HTTP 请求头不是有效 UTF-8"));
        }
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
