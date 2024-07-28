use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

fn main() {
    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                println!("accepted new connection");
                let buf_reader = BufReader::new(&mut stream);

                let request_line = buf_reader.lines().next().unwrap().unwrap();
                let mut parts = request_line.split_whitespace().skip(1);
                let target = parts.next().unwrap();
                let response : String = if target == "/" {
                    "HTTP/1.1 200 OK\r\n\r\n".into()
                } else if target.starts_with("/echo/") {
                    let str = &target[6..];
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
