use std::io::Write;
use std::net::TcpListener;
use nom::AsBytes;

fn main() {
    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                println!("accepted new connection");
                stream.write_all("HTTP/1.1 200 OK\r\n\r\n".as_bytes()).expect("200");
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}
