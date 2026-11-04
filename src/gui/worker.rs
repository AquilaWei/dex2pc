use std::io::{self, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use super::{Event, Status};
use crate::runner::{CommandOutput, CommandRunner, RunningCommand, SystemRunner, TimedOutput};
use crate::{scrcpy::Options, start};

/// 每個執行中的指令各有停止 channel；UI 保留最新 sender，完全不共享行程或可變狀態。
pub struct SessionRunner<R = SystemRunner> {
    events: Sender<Event>,
    backend: R,
}
impl SessionRunner {
    pub fn new(events: Sender<Event>) -> Self {
        Self {
            events,
            backend: SystemRunner,
        }
    }
}
struct ControlledChild {
    inner: Box<dyn RunningCommand>,
    stop: Receiver<()>,
}
impl RunningCommand for ControlledChild {
    fn try_wait(&mut self) -> io::Result<Option<CommandOutput>> {
        if self.stop.try_recv().is_ok() {
            self.inner.kill()?;
            return Ok(Some(CommandOutput {
                code: None,
                stdout: String::new(),
                stderr: String::new(),
            }));
        }
        self.inner.try_wait()
    }
    fn kill(&mut self) -> io::Result<()> {
        self.inner.kill()
    }
}
impl<R: CommandRunner> CommandRunner for SessionRunner<R> {
    fn spawn(&self, program: &str, args: &[&str]) -> io::Result<Box<dyn RunningCommand>> {
        let child = self.backend.spawn(program, args)?;
        let (tx, rx) = mpsc::channel();
        // 配對參數不送到事件或 log；UI 只需要停止 handle 與是否已啟動投影。
        self.events
            .send(Event::Command(
                tx,
                program == "scrcpy" && args != ["--version"],
            ))
            .map_err(|_| io::Error::other("視窗已結束"))?;
        Ok(Box::new(ControlledChild {
            inner: child,
            stop: rx,
        }))
    }
    fn run(&self, program: &str, args: &[&str]) -> io::Result<CommandOutput> {
        let timeout = if program == "scrcpy" && args != ["--version"] {
            Duration::MAX
        } else {
            Duration::from_secs(15)
        };
        match self.run_timeout(program, args, timeout)? {
            TimedOutput::Exited(output) => Ok(output),
            TimedOutput::TimedOut => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "指令逾時，請確認手機連線",
            )),
        }
    }
    fn run_timeout(
        &self,
        program: &str,
        args: &[&str],
        timeout: Duration,
    ) -> io::Result<TimedOutput> {
        let mut child = self.spawn(program, args)?;
        let began = Instant::now();
        loop {
            if let Some(output) = child.try_wait()? {
                return Ok(TimedOutput::Exited(output));
            }
            if began.elapsed() >= timeout {
                child.kill()?;
                return Ok(TimedOutput::TimedOut);
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
}

/// 將原本 doctor/start 的輸出逐行送回 UI；不讓背景工作直接存取 egui。
struct Messages(Sender<Event>);
impl Write for Messages {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .send(Event::Log(String::from_utf8_lossy(bytes).into_owned()))
            .map_err(|_| io::Error::other("視窗已結束"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub fn session(events: Sender<Event>, mut options: Options) {
    thread::spawn(move || {
        let runner = SessionRunner::new(events.clone());
        options.video_bit_rate = Some(options.quality.unwrap_or_default().bitrate().into());
        let mut out = Messages(events.clone());
        let watch = start::Watch::default();
        let result = match options.mode.unwrap_or_default() {
            crate::preferences::Mode::Usb => start::run(
                &runner,
                &options,
                None,
                Some(&watch),
                Path::new("/sys"),
                &mut out,
            ),
            crate::preferences::Mode::Wifi => match options.wifi_address.as_deref() {
                Some(address) => start::run_wifi(
                    &runner,
                    &options,
                    address,
                    Some(&watch),
                    Path::new("/sys"),
                    &mut out,
                ),
                None => Err(io::Error::other("未設定 wifi_address，請先連線")),
            },
        };
        let status = match result {
            Ok(0) => Status::Idle,
            Ok(_) | Err(_) => Status::Error,
        };
        let _ = events.send(Event::Finished(
            status,
            result.err().map(|err| err.to_string()),
        ));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;
    struct FakeChild {
        killed: Rc<Cell<bool>>,
    }
    impl RunningCommand for FakeChild {
        fn try_wait(&mut self) -> io::Result<Option<CommandOutput>> {
            Ok(None)
        }
        fn kill(&mut self) -> io::Result<()> {
            self.killed.set(true);
            Ok(())
        }
    }
    struct FakeRunner {
        killed: Rc<Cell<bool>>,
    }
    impl CommandRunner for FakeRunner {
        fn run(&self, _: &str, _: &[&str]) -> io::Result<CommandOutput> {
            panic!("GUI 必須使用可停止的子行程");
        }
        fn spawn(&self, _: &str, _: &[&str]) -> io::Result<Box<dyn RunningCommand>> {
            Ok(Box::new(FakeChild {
                killed: self.killed.clone(),
            }))
        }
    }
    #[test]
    fn ui_stop_kills_scrcpy_through_running_command() {
        let (events, rx) = mpsc::channel();
        let killed = Rc::new(Cell::new(false));
        let runner = SessionRunner {
            events,
            backend: FakeRunner {
                killed: killed.clone(),
            },
        };
        let mut child = runner
            .spawn("scrcpy", &["--new-display=1920x1080/160"])
            .unwrap();
        let Event::Command(stop, _) = rx.recv().unwrap() else {
            panic!("未取得停止 handle");
        };
        stop.send(()).unwrap();
        assert_eq!(child.try_wait().unwrap().unwrap().code, None);
        assert!(killed.get());
    }
    #[test]
    fn gui_probe_timeout_kills_child() {
        let killed = Rc::new(Cell::new(false));
        // 保留 receiver，避免 UI 已結束的錯誤蓋過逾時。
        let (events, _rx) = mpsc::channel();
        let runner = SessionRunner {
            events,
            backend: FakeRunner {
                killed: killed.clone(),
            },
        };
        assert_eq!(
            runner
                .run_timeout("adb", &["devices", "-l"], Duration::ZERO)
                .unwrap(),
            TimedOutput::TimedOut
        );
        assert!(killed.get());
    }
}
