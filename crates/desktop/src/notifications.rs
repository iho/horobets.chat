use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
};
pub struct Notifications {
    child: Child,
    input: ChildStdin,
    pub events: async_channel::Receiver<Value>,
}
impl Notifications {
    pub fn start() -> anyhow::Result<Self> {
        #[cfg(target_os = "macos")]
        let program = {
            let executable = std::env::current_exe()?;
            let bundled = executable.parent().unwrap().join("NotificationAgent");
            if bundled.exists() {
                bundled
            } else {
                std::path::PathBuf::from(env!("MESSENGER_NOTIFICATION_AGENT"))
            }
        };
        #[cfg(not(target_os = "macos"))]
        let program = std::path::PathBuf::from("horobets-notification-agent");
        let mut child = Command::new(program)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (tx, events) = async_channel::bounded(64);
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines().map_while(Result::ok) {
                if let Ok(value) = serde_json::from_str(&line)
                    && tx.send_blocking(value).is_err()
                {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input,
            events,
        })
    }
    pub fn send(&mut self, value: Value) {
        let _ = writeln!(self.input, "{value}");
        let _ = self.input.flush();
    }
}
impl Drop for Notifications {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
