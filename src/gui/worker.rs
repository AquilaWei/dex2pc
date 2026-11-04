use std::io::{self, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use super::{Event, Status};
use crate::runner::{CommandOutput, CommandRunner, RunningCommand, SystemRunner, TimedOutput};
use crate::{scrcpy::Options, start};

/// 每個子行程由自己的監控執行緒持有；UI 以 channel 停止，不受 watchdog 輪詢間隔影響。
pub struct SessionRunner<R = SystemRunner> {
    events: Sender<Event>,
    backend: R,
}
impl<R> SessionRunner<R> {
    #[cfg(test)]
    pub(super) fn with_backend(events: Sender<Event>, backend: R) -> Self {
        Self { events, backend }
    }
}
impl SessionRunner {
    pub fn new(events: Sender<Event>) -> Self {
        Self {
            events,
            backend: SystemRunner,
        }
    }
}
enum Control {
    Stop(Sender<io::Result<()>>),
}

pub(super) struct StopHandle {
    control: Sender<Control>,
    done: Receiver<()>,
}
impl StopHandle {
    pub(super) fn send(&self, (): ()) -> io::Result<()> {
        stop_child(&self.control)
    }
    pub(super) fn finished(&self) -> bool {
        !matches!(self.done.try_recv(), Err(mpsc::TryRecvError::Empty))
    }
}
fn stop_child(control: &Sender<Control>) -> io::Result<()> {
    let (tx, rx) = mpsc::channel();
    if control.send(Control::Stop(tx)).is_err() {
        return Ok(());
    }
    rx.recv()
        .map_err(|_| io::Error::other("子行程監控已結束"))?
}
struct ControlledChild {
    output: Receiver<io::Result<CommandOutput>>,
    control: Sender<Control>,
}
impl RunningCommand for ControlledChild {
    fn try_wait(&mut self) -> io::Result<Option<CommandOutput>> {
        match self.output.try_recv() {
            Ok(output) => output.map(Some),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(io::Error::other("子行程監控已結束")),
        }
    }
    fn kill(&mut self) -> io::Result<()> {
        stop_child(&self.control)
    }
}
impl Drop for ControlledChild {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}

// 子行程留在建立它的執行緒，不要求 RunningCommand 或既有 CLI fake 改成 Send。
fn supervise(
    child: &mut dyn RunningCommand,
    control: &Receiver<Control>,
    output: &Sender<io::Result<CommandOutput>>,
) {
    loop {
        match child.try_wait() {
            Ok(None) => (),
            result => {
                let _ = output.send(result.map(Option::unwrap));
                return;
            }
        }
        match control.recv_timeout(Duration::from_millis(25)) {
            Ok(Control::Stop(ack)) => {
                let result = child.kill();
                let message = match &result {
                    Ok(()) => Ok(CommandOutput {
                        code: None,
                        stdout: String::new(),
                        stderr: String::new(),
                    }),
                    Err(err) => Err(io::Error::new(err.kind(), err.to_string())),
                };
                // 先交付結果再確認停止，呼叫端收到確認後就能讀到 Interrupted。
                let _ = output.send(message);
                let _ = ack.send(result);
                return;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = child.kill();
                return;
            }
        }
    }
}
impl<R: CommandRunner + Clone + Send + 'static> CommandRunner for SessionRunner<R> {
    fn spawn(&self, program: &str, args: &[&str]) -> io::Result<Box<dyn RunningCommand>> {
        let backend = self.backend.clone();
        let program_owned = program.to_owned();
        let args_owned: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
        let (control, commands) = mpsc::channel();
        let (result, output) = mpsc::channel();
        let (ready, started) = mpsc::channel();
        let (done, finished) = mpsc::channel::<()>();
        thread::spawn(move || {
            let _done = done;
            let args: Vec<&str> = args_owned.iter().map(String::as_str).collect();
            match backend.spawn(&program_owned, &args) {
                Ok(mut child) => {
                    if ready.send(Ok(())).is_ok() {
                        supervise(child.as_mut(), &commands, &result);
                    } else {
                        let _ = child.kill();
                    }
                }
                Err(err) => {
                    let _ = ready.send(Err(err));
                }
            }
        });
        started
            .recv()
            .map_err(|_| io::Error::other("無法啟動子行程監控"))??;
        let child = ControlledChild {
            output,
            control: control.clone(),
        };
        // 配對參數不送到事件或 log；UI 僅持有可停止且可辨識結束的 handle。
        self.events
            .send(Event::Command(
                StopHandle {
                    control,
                    done: finished,
                },
                program == "scrcpy" && args != ["--version"],
            ))
            .map_err(|_| io::Error::other("視窗已結束"))?;
        Ok(Box::new(child))
    }
    fn run(&self, program: &str, args: &[&str]) -> io::Result<CommandOutput> {
        command_result(program, args, |timeout| {
            self.run_timeout(program, args, timeout)
        })
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

// 將逾時政策和實際計時分開，測試才能模擬超過十五秒而不真的等待。
fn command_result(
    program: &str,
    args: &[&str],
    execute: impl FnOnce(Duration) -> io::Result<TimedOutput>,
) -> io::Result<CommandOutput> {
    // USB 重連和 CLI 一樣持續等待；監控執行緒仍能即時接受 Stop。
    let timeout = if (program == "scrcpy" && args != ["--version"])
        || (program == "adb" && args.last() == Some(&"wait-for-device"))
    {
        Duration::MAX
    } else {
        Duration::from_secs(15)
    };
    match execute(timeout)? {
        TimedOutput::Exited(output) => Ok(output),
        TimedOutput::TimedOut => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "指令逾時，請確認手機連線",
        )),
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
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    #[derive(Default)]
    struct KillFlag(AtomicBool);
    impl KillFlag {
        fn get(&self) -> bool {
            self.0.load(Ordering::SeqCst)
        }
        fn set(&self, value: bool) {
            self.0.store(value, Ordering::SeqCst);
        }
    }
    struct FakeChild {
        killed: Arc<KillFlag>,
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
    #[derive(Clone)]
    struct FakeRunner {
        killed: Arc<KillFlag>,
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
        let killed = Arc::new(KillFlag::default());
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
        let killed = Arc::new(KillFlag::default());
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
    struct SixteenSecondRunner;
    impl CommandRunner for SixteenSecondRunner {
        fn run(&self, _: &str, _: &[&str]) -> io::Result<CommandOutput> {
            unreachable!()
        }
        fn run_timeout(&self, _: &str, _: &[&str], timeout: Duration) -> io::Result<TimedOutput> {
            // 虛擬指令在第十六秒才完成，不使用真實時鐘或 sleep。
            Ok(if timeout < Duration::from_secs(16) {
                TimedOutput::TimedOut
            } else {
                TimedOutput::Exited(CommandOutput {
                    code: Some(0),
                    stdout: String::new(),
                    stderr: String::new(),
                })
            })
        }
    }
    #[test]
    fn wait_for_device_longer_than_fifteen_seconds_is_not_killed() {
        let fake = SixteenSecondRunner;
        let result = command_result("adb", &["-s", "phone", "wait-for-device"], |timeout| {
            fake.run_timeout("adb", &["-s", "phone", "wait-for-device"], timeout)
        });
        assert_eq!(result.unwrap().code, Some(0));
    }
}
