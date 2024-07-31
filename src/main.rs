use clap::Parser;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::str::FromStr;
use threadpool::ThreadPool;

#[derive(Eq, PartialEq, Debug)]
enum Method {
    Get,
    Post,
}

impl FromStr for Method {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use Method::*;
        match s {
            "GET" => Ok(Get),
            "POST" => Ok(Post),
            _ => Err(())
        }
    }
}

fn read_until<R: BufRead + ?Sized>(r: &mut R, delim: [u8; 2], buf: &mut Vec<u8>) -> std::io::Result<usize> {
    let mut read = 0;
    loop {
        let (done, used) = {
            let available = match r.fill_buf() {
                Ok(n) => n,
                Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };
            match available.windows(delim.len()).position(|w| w == &delim) {
                Some(i) => {
                    buf.extend_from_slice(&available[..i + delim.len()]);
                    (true, i + delim.len())
                }
                None => {
                    buf.extend_from_slice(available);
                    (false, available.len())
                }
            }
        };
        r.consume(used);
        read += used;
        if done || used == 0 {
            return Ok(read);
        }
    }
}

#[derive(Debug)]
struct Request {
    method: Method,
    target: String,
    version: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn read_from_stream(stream: &mut TcpStream) -> std::io::Result<Request> {
    let mut buf_reader = BufReader::new(stream);
    let mut buf = Vec::with_capacity(256);
    let n = read_until(&mut buf_reader, *b"\r\n", &mut buf)?;
    let request_line = &buf[..n.saturating_sub(2)];
    let mut parts = request_line.split(|b| *b == b' ');
    let method = parts.next().unwrap();
    let method = std::str::from_utf8(method).unwrap().parse().unwrap();
    let target = parts.next().unwrap();
    let target = String::from_utf8(target.to_vec()).unwrap();
    let version = parts.next().unwrap();
    let version = String::from_utf8(version.to_vec()).unwrap();
    buf.clear();

    let mut headers = HashMap::new();
    loop {
        let n = read_until(&mut buf_reader, *b"\r\n", &mut buf).unwrap();
        if n == 2 {
            buf.clear();
            break;
        }
        let line = std::str::from_utf8(&buf[..n.saturating_sub(2)]).unwrap();
        let (key, value) = line.split_once(": ").unwrap();
        headers.insert(key.to_string(), value.to_string());
        buf.clear();
    }
    let mut body = vec![];
    match headers.get("Content-Length") {
        None => {}
        Some(value) => {
            let len: usize = value.parse().unwrap();
            buf.resize(2048, 0);
            loop {
                let n = buf_reader.read(&mut buf).unwrap();
                body.extend_from_slice(&buf[..n]);
                if body.len() == len { break; }
            }
        }
    }

    Ok(Request { method, target, version, headers, body })
}

enum StatusCode {
    Ok,
    Created,
    BadRequest,
    NotFound,
}

impl StatusCode {
    fn code(&self) -> usize {
        match self {
            StatusCode::Ok => 200,
            StatusCode::Created => 201,
            StatusCode::BadRequest => 400,
            StatusCode::NotFound => 404,
        }
    }

    fn canonical_reason(&self) -> &'static str {
        match self {
            StatusCode::Ok => "OK",
            StatusCode::Created => "Created",
            StatusCode::BadRequest => "Bad Request",
            StatusCode::NotFound => "Not Found",
        }
    }
}

struct Response {
    version: String,
    status: StatusCode,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl Response {
    fn with_code(status_code: StatusCode) -> Self {
        Self {
            version: "HTTP/1.1".to_string(),
            status: status_code,
            headers: Default::default(),
            body: vec![],
        }
    }
    fn write(&self, writer: &mut impl Write) -> std::io::Result<()> {
        writer.write_all(format!("{} {} {}\r\n", self.version, self.status.code(), self.status.canonical_reason()).as_bytes())?;
        for (key, value) in &self.headers {
            let mut line = key.clone();
            line.push_str(": ");
            line.push_str(value);
            line.push_str("\r\n");
            writer.write_all(line.as_bytes())?;
        }
        writer.write_all(b"\r\n")?;
        if let Some(len) = self.headers.get("Content-Length") {
            let len: usize = len.parse().unwrap();
            assert_eq!(len, self.body.len());
        } else {
            assert!(self.body.is_empty());
        }
        writer.write_all(&self.body)?;
        Ok(())
    }
}

fn handle_connection(mut stream: TcpStream, directory: Option<PathBuf>) {
    let request = read_from_stream(&mut stream).unwrap();
    println!("{:?}", request);

    let mut response = match (request.method, request.target.as_str()) {
        (Method::Get, "/") => Response::with_code(StatusCode::Ok),
        (Method::Get, "/user-agent") => {
            let user_agent = request.headers.get("User-Agent").unwrap();
            let mut response = Response::with_code(StatusCode::Ok);
            response.headers.insert("Content-Type".into(), "text/plain".into());
            response.headers.insert("Content-Length".into(), format!("{}", user_agent.len()));
            response.body = user_agent.as_bytes().to_vec();
            response
        }
        (Method::Get, target) if target.starts_with("/echo/") => {
            let message = target.trim_start_matches("/echo/");
            let mut response = Response::with_code(StatusCode::Ok);
            response.headers.insert("Content-Type".into(), "text/plain".into());
            response.headers.insert("Content-Length".into(), format!("{}", message.len()));
            response.body = message.as_bytes().to_vec();
            response
        }
        (Method::Get, target) if target.starts_with("/files/") => {
            let file_name = target.trim_start_matches("/files/");
            if let Some(Ok(content)) = directory.map(|mut path| {
                path.push(file_name);
                std::fs::read(path)
            }) {
                let mut response = Response::with_code(StatusCode::Ok);
                response.headers.insert("Content-Type".into(), "application/octet-stream".into());
                response.headers.insert("Content-Length".into(), format!("{}", content.len()));
                response.body = content;
                response
            } else {
                Response::with_code(StatusCode::NotFound)
            }
        }
        (Method::Post, target) if target.starts_with("/files/") => {
            let file_name = target.trim_start_matches("/files/");
            if let Some(Ok(mut file)) = directory.map(|mut path| {
                path.push(file_name);
                File::create_new(path)
            }) {
                file.write_all(&request.body).unwrap();
                Response::with_code(StatusCode::Created)
            } else {
                Response::with_code(StatusCode::BadRequest)
            }
        }
        _ => Response::with_code(StatusCode::NotFound),
    };


    let encodings: Vec<_> = request.headers.get("Accept-Encoding").map(|value| value.split(", ").collect()).unwrap_or_default();
    if encodings.contains(&"gzip") {
        response.headers.insert("Content-Encoding".into(), "gzip".into());
    }

    let mut buf_writer = BufWriter::new(&mut stream);
    response.write(&mut buf_writer).unwrap();
}

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    directory: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();

    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();

    let pool = ThreadPool::new(4);

    for stream in listener.incoming() {
        let stream = stream.unwrap();
        let directory = args.directory.clone();
        pool.execute(move || {
            handle_connection(stream, directory);
        });
    }
}
