use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

fn main() {
    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
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

                let response : String = if target == "/" {
                    "HTTP/1.1 200 OK\r\n\r\n".into()
                } else if target == "/user-agent" {
                    let str = headers.get("user-agent").cloned().unwrap_or_default();
                    format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}", str.len(), str)
                } else if target.starts_with("/echo/") {
                    let str = target.trim_start_matches("/echo/");
                    format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}", str.len(), str)
                } else {
                    "HTTP/1.1 404 Not Found\r\n\r\n".into()
                };

                stream.write_all(response.as_bytes()).unwrap()
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}
