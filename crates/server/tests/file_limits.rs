#![cfg(unix)]

use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    os::unix::process::CommandExt,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn inherited_low_limit_does_not_exhaust_server_after_256_connections() {
    let directory = tempfile::tempdir().unwrap();
    let address = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_messenger-server"));
    command
        .env("MESSENGER_DATA_DIR", directory.path())
        .env("MESSENGER_LISTEN", address.to_string())
        .env(
            "MESSENGER_JWT_SECRET",
            "file-limit-regression-test-secret-12345678",
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: this child-only hook uses async-signal-safe libc calls to change
    // its own soft limit, without changing the test runner's descriptor budget.
    unsafe {
        command.pre_exec(|| {
            let mut limits = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limits) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if limits.rlim_max < 512 {
                return Err(std::io::Error::other(
                    "test needs a hard file limit of at least 512",
                ));
            }
            limits.rlim_cur = 256;
            if libc::setrlimit(libc::RLIMIT_NOFILE, &limits) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut server = Server(command.spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if TcpStream::connect(address).is_ok() {
            break;
        }
        assert!(
            server.0.try_wait().unwrap().is_none(),
            "server exited during startup"
        );
        assert!(Instant::now() < deadline, "server did not start");
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut connections = Vec::new();
    for _ in 0..300 {
        let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        let mut response = BufReader::new(stream);
        let mut status = String::new();
        response.read_line(&mut status).unwrap();
        assert!(status.starts_with("HTTP/1.1 200"), "{status}");
        connections.push(response);
    }
    assert_eq!(connections.len(), 300);
}
