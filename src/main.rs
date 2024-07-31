use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use clap::Parser;
use threadpool::ThreadPool;

fn handle_connection(mut stream: TcpStream, directory: PathBuf) {
    println!("accepted new connection");
    let buf_reader = BufReader::new(&mut stream);
    let mut lines = buf_reader.lines();
    let request_line = lines.next().unwrap().unwrap();
    let mut parts = request_line.split_whitespace().skip(1);
    let target = parts.next().unwrap();

    let mut headers = HashMap::new();
    let mut line = lines.next().unwrap().unwrap();
    while !line.is_empty() {
        let (key, value) = line.split_once(": ").unwrap();
        println!("header {}: {}", key, value);
        headers.insert(key.to_lowercase(), value.to_string());
        line = lines.next().unwrap().unwrap();
    }
    //let body = lines.next().unwrap().unwrap();

    if target == "/" {
        stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").unwrap()
    } else if target == "/user-agent" {
        let str = headers.get("user-agent").cloned().unwrap_or_default();
        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}", str.len(), str).as_bytes()).unwrap()
    } else if target.starts_with("/echo/") {
        let str = target.trim_start_matches("/echo/");
        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}", str.len(), str).as_bytes()).unwrap()
    } else if target.starts_with("/files/") {
        let file_name = target.trim_start_matches("/files/");
        let mut path = directory;
        path.push(file_name);
        if let Ok(mut file) =  File::open(path) {
            let len = file.metadata().unwrap().len();
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {len}\r\n\r\n").as_bytes()).unwrap();
            let mut buf = [0; 4096];
            loop {
                let n = file.read(&mut buf).unwrap();
                if n == 0 { break; }
                stream.write_all(&buf[..n]).unwrap();
            }
        } else {
            stream.write_all(b"HTTP/1.1 404 Not Found\r\n\r\n").unwrap();
        }
    } else {
        stream.write_all(b"HTTP/1.1 404 Not Found\r\n\r\n").unwrap()
    };
}

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    directory: PathBuf,
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
