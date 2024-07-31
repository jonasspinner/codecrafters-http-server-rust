use clap::Parser;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
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

fn handle_connection(mut stream: TcpStream, directory: Option<PathBuf>) {
    let request = read_from_stream(&mut stream).unwrap();
    println!("{:?}", request);

    match (request.method, request.target.as_str()) {
        (Method::Get, "/") => { stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").unwrap() }
        (Method::Get, "/user-agent") => {
            let str = request.headers.get("User-Agent").unwrap();
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}", str.len(), str).as_bytes()).unwrap()
        }
        (Method::Get, target) if target.starts_with("/echo/") => {
            let str = target.trim_start_matches("/echo/");
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}", str.len(), str).as_bytes()).unwrap()
        }
        (Method::Get, target) if target.starts_with("/files/") => {
            let file_name = target.trim_start_matches("/files/");
            if let Some(Ok(mut file)) = directory.map(|mut path| {
                path.push(file_name);
                File::open(path)
            }) {
                let len = file.metadata().unwrap().len();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {len}\r\n\r\n").as_bytes()).unwrap();
                let mut buf = [0; 4096];
                loop {
                    let n = file.read(&mut buf).unwrap();
                    if n == 0 {
                        break;
                    }
                    stream.write_all(&buf[..n]).unwrap();
                }
            } else {
                stream.write_all(b"HTTP/1.1 404 Not Found\r\n\r\n").unwrap();
            }
        }
        (Method::Post, target) if target.starts_with("/files/") => {
            let file_name = target.trim_start_matches("/files/");
            if let Some(Ok(mut file)) = directory.map(|mut path| {
                path.push(file_name);
                File::create_new(path)
            }) {
                file.write_all(&request.body).unwrap();
                stream.write_all(b"HTTP/1.1 201 Created\r\n\r\n").unwrap()
            } else {
                stream.write_all(b"HTTP/1.1 500 \r\n\r\n").unwrap()
            }
        }
        _ => {
            stream.write_all(b"HTTP/1.1 404 Not Found\r\n\r\n").unwrap()
        }
    }
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
