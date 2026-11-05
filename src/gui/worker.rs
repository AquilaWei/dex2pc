use std::io::{self, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use super::text::{self, Key};
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
        .map_err(|_| io::Error::other(text::zh(Key::MonitorEnded)))?
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
            Err(mpsc::TryRecvError::Disconnected) => {
                Err(io::Error::other(text::zh(Key::MonitorEnded)))
            }
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
            .map_err(|_| io::Error::other(text::zh(Key::MonitorFailed)))??;
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
            .map_err(|_| io::Error::other(text::zh(Key::WindowEnded)))?;
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
            text::zh(Key::CommandTimeout),
        )),
    }
}

/// 將原本 doctor/start 的輸出逐行送回 UI；不讓背景工作直接存取 egui。
pub(super) struct Messages {
    events: Sender<Event>,
    recent: Vec<u8>,
}
impl Messages {
    pub(super) fn new(events: Sender<Event>) -> Self {
        Self {
            events,
            recent: Vec::new(),
        }
    }
}
impl Write for Messages {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.events
            .send(Event::Log(String::from_utf8_lossy(bytes).into_owned()))
            .map_err(|_| io::Error::other(text::zh(Key::WindowEnded)))?;
        self.recent.extend_from_slice(bytes);
        // Keep only recent diagnostics; watch sessions can run indefinitely.
        if self.recent.len() > 32_768 {
            self.recent.drain(..self.recent.len() - 32_768);
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn session_result(
    result: io::Result<u8>,
    mode: crate::preferences::Mode,
    log: &str,
) -> (Status, Option<String>) {
    let reason = match result {
        Ok(0) => return (Status::Idle, None),
        Ok(code) => text::message(Key::SessionExit, &[("{code}", &code.to_string())]),
        Err(err) => text::message(Key::SessionIo, &[("{detail}", &err.to_string())]),
    };
    let hint = if log.contains("Network is unreachable") || log.contains("No route to host") {
        NETWORK_HINT
    } else if log.contains("尚未授權") || log.contains("unauthorized") {
        text::zh(Key::Unauthorized)
    } else if log.contains("scrcpy 以 exit code") || log.contains("無法執行 scrcpy") {
        text::zh(Key::ScrcpyHint)
    } else if mode == crate::preferences::Mode::Wifi {
        NETWORK_HINT
    } else {
        text::zh(Key::UsbRecovery)
    };
    let mut message = format!("{reason}{hint}");
    let mut lines = log
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty() && *line != "環境未就緒，未啟動 scrcpy；請依上面的訊息處理後再執行"
        })
        .rev();
    if let Some(last) = lines.next() {
        message.push('\n');
        // run_wifi ends with a summary; retain adb's preceding network reason too.
        if last.starts_with("Wi-Fi 連線失敗：")
            && let Some(reason) = lines.next()
        {
            message.push_str(reason);
            message.push('\n');
        }
        message.push_str(last);
    }
    (Status::Error, Some(message))
}

pub(super) const NETWORK_HINT: &str = text::zh(Key::Network);

pub fn session(events: Sender<Event>, mut options: Options) {
    thread::spawn(move || {
        let runner = SessionRunner::new(events.clone());
        options.video_bit_rate = Some(options.quality.unwrap_or_default().bitrate().into());
        let mut out = Messages::new(events.clone());
        let watch = start::Watch {
            max_failures: if options.mode == Some(crate::preferences::Mode::Wifi) {
                start::WIFI_MAX_FAILURES
            } else {
                start::Watch::default().max_failures
            },
            ..start::Watch::default()
        };
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
                None => Err(io::Error::other(text::zh(Key::NoAddress))),
            },
        };
        let (status, error) = session_result(
            result,
            options.mode.unwrap_or_default(),
            &String::from_utf8_lossy(&out.recent),
        );
        let _ = events.send(Event::Finished(status, error));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    #[test]
    fn nonzero_exit_always_includes_a_recovery_message() {
        assert_eq!(
            session_result(Ok(7), crate::preferences::Mode::Usb, "").1,
            Some("投影失敗（exit code 7）。請檢查手機連線與環境，依下方訊息處理後重試。".into())
        );
    }
    #[test]
    fn unreachable_wifi_reports_changed_ip_and_keeps_adb_reason() {
        assert_eq!(
            session_result(Ok(1), crate::preferences::Mode::Wifi,
                "failed to connect to '192.168.12.137:5555': Network is unreachable\nWi-Fi 連線失敗：192.168.12.137:5555\n").1,
            Some("投影失敗（exit code 1）。手機與電腦可能位於不同網路，或手機 IP 已變更。請確認同一 Wi-Fi 並開啟無線偵錯，再重新掃描或重新配對。\nfailed to connect to '192.168.12.137:5555': Network is unreachable\nWi-Fi 連線失敗：192.168.12.137:5555".into())
        );
    }
    #[test]
    fn no_route_reports_changed_ip_and_current_wireless_debugging_address() {
        assert_eq!(
            session_result(Ok(1), crate::preferences::Mode::Wifi, "No route to host\n").1,
            Some("投影失敗（exit code 1）。手機與電腦可能位於不同網路，或手機 IP 已變更。請確認同一 Wi-Fi 並開啟無線偵錯，再重新掃描或重新配對。\nNo route to host".into())
        );
    }
    #[test]
    fn unauthorized_phone_explains_how_to_allow_debugging() {
        assert_eq!(
            session_result(Ok(1), crate::preferences::Mode::Usb,
                "裝置 phone 尚未授權：請在手機上按「允許 USB 偵錯」\n環境未就緒，未啟動 scrcpy；請依上面的訊息處理後再執行\n").1,
            Some("投影失敗（exit code 1）。請解鎖手機並按「允許 USB 偵錯」。\n裝置 phone 尚未授權：請在手機上按「允許 USB 偵錯」".into())
        );
    }
    #[test]
    fn scrcpy_failure_keeps_exit_code_and_last_diagnostic() {
        assert_eq!(
            session_result(Ok(5), crate::preferences::Mode::Usb,
                "scrcpy 以 exit code 5 結束：\nERROR: Could not create display\n").1,
            Some("投影失敗（exit code 5）。scrcpy 啟動或執行失敗；請確認版本至少為 3.2，並依下方訊息檢查手機連線後重試。\nERROR: Could not create display".into())
        );
    }
    #[test]
    fn io_error_has_a_recovery_message() {
        assert_eq!(
            session_result(Err(io::Error::other("指令逾時，請確認手機連線")), crate::preferences::Mode::Wifi, "").1,
            Some("投影失敗：指令逾時，請確認手機連線。手機與電腦可能位於不同網路，或手機 IP 已變更。請確認同一 Wi-Fi 並開啟無線偵錯，再重新掃描或重新配對。".into())
        );
    }
    #[test]
    fn successful_session_has_no_error_message() {
        assert_eq!(
            session_result(Ok(0), crate::preferences::Mode::Wifi, "old failure"),
            (Status::Idle, None)
        );
    }
    #[test]
    fn streamed_log_fragments_are_retained_for_the_final_error() {
        let (events, _rx) = mpsc::channel();
        let mut out = Messages::new(events);
        out.write_all(b"Network is ").unwrap();
        out.write_all(b"unreachable\n").unwrap();
        assert_eq!(
            String::from_utf8(out.recent).unwrap(),
            "Network is unreachable\n"
        );
    }
    #[test]
    fn initial_wifi_connect_has_a_fifteen_second_timeout() {
        let result = command_result("adb", &["connect", "192.168.12.137:5555"], |timeout| {
            assert_eq!(timeout, Duration::from_secs(15));
            Ok(TimedOutput::TimedOut)
        });
        assert_eq!(result.unwrap_err().to_string(), "指令逾時，請確認手機連線");
    }
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
