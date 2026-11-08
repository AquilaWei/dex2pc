use std::collections::BTreeSet;
use std::io::{self, Write};
use std::thread;
use std::time::{Duration, Instant};

use crate::runner::{CommandOutput, CommandRunner, RunningCommand, TimedOutput};

/// 只選 shell 擁有的 scrcpy 虛擬顯示器，並合併 base／override 的重複紀錄。
fn displays(dump: &str) -> BTreeSet<u32> {
    dump.lines()
        .filter(|line| {
            line.contains("DisplayInfo{")
                && line.contains("type VIRTUAL,")
                && line.contains("uniqueId \"virtual:com.android.shell,2000,scrcpy,")
                && line.contains("owner com.android.shell (uid 2000)")
        })
        .filter_map(|line| {
            line.split_once("displayId ")?
                .1
                .split(',')
                .next()?
                .parse()
                .ok()
        })
        .filter(|id| *id != 0)
        .collect()
}

fn new_display(before: &BTreeSet<u32>, dump: &str) -> Result<Option<u32>, String> {
    let current = displays(dump);
    let mut added = current.difference(before);
    match (added.next(), added.next()) {
        (Some(id), None) => Ok(Some(*id)),
        (None, _) => Ok(None),
        _ => Err("多個新的 scrcpy 顯示器，無法安全選擇".into()),
    }
}

fn mode_args(serial: &str, id: &str, mode: &str) -> Vec<String> {
    [
        "-s",
        serial,
        "shell",
        "wm",
        "set-display-windowing-mode",
        "-d",
        id,
        mode,
    ]
    .map(str::to_owned)
    .to_vec()
}

/// 包裝 CLI 與 GUI 的投影流程，讓每次啟動或重連都重新比對顯示器，避免修改其他工作階段。
/// 顯示器辨識或模式設定失敗只警告並繼續投影；子行程啟動、監測或輸出寫入失敗則回傳 I/O 錯誤。
pub struct Projection<'a> {
    runner: &'a dyn CommandRunner,
    serial: &'a str,
    enabled: bool,
}
impl<'a> Projection<'a> {
    /// 綁定已選定的手機與視窗模式，讓兩條啟動路徑採用相同的顯示器設定策略。
    /// 此建構函式不執行指令、不會失敗；執行階段的辨識或設定失敗會警告，投影 I/O 錯誤則回傳呼叫端。
    pub fn new(runner: &'a dyn CommandRunner, serial: &'a str, enabled: bool) -> Self {
        Self {
            runner,
            serial,
            enabled,
        }
    }

    fn report_mode(&self, out: &mut dyn Write) -> io::Result<()> {
        writeln!(
            out,
            "DeX 視窗模式：{}",
            if self.enabled {
                "freeform"
            } else {
                "fullscreen"
            }
        )
    }

    fn adb(&self, args: &[&str]) -> Result<String, String> {
        // 無線 adb 有延遲，且 dumpsys display 輸出很大，單次指令保留五秒。
        match self.runner.run_timeout("adb", args, Duration::from_secs(5)) {
            Ok(TimedOutput::Exited(output)) if output.success() => Ok(output.stdout),
            Ok(TimedOutput::Exited(output)) => Err(format!(
                "adb exit {:?}: {} {}",
                output.code,
                output.stdout.trim(),
                output.stderr.trim()
            )),
            Ok(TimedOutput::TimedOut) => Err("adb 逾時（5 秒）".into()),
            Err(err) => Err(err.to_string()),
        }
    }

    fn snapshot(&self) -> Result<BTreeSet<u32>, String> {
        self.adb(&["-s", self.serial, "shell", "dumpsys", "display"])
            .map(|s| displays(&s))
    }

    fn configure(&self, before: &BTreeSet<u32>) -> Result<(), String> {
        // scrcpy 啟動後才註冊顯示器，保留十五秒等待手機完成建立。
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let dump = self.adb(&["-s", self.serial, "shell", "dumpsys", "display"])?;
            if let Some(id) = new_display(before, &dump)? {
                // Android 可能依 uniqueId 保留模式覆寫，停用時要明確還原全螢幕。
                let mode_id = if self.enabled { "5" } else { "1" };
                let expected = if self.enabled {
                    "freeform"
                } else {
                    "fullscreen"
                };
                let args = mode_args(self.serial, &id.to_string(), mode_id);
                let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                self.adb(&refs)?;
                let mode = self.adb(&[
                    "-s",
                    self.serial,
                    "shell",
                    "wm",
                    "get-display-windowing-mode",
                    "-d",
                    &id.to_string(),
                ])?;
                if !mode.contains(&format!("display windowing mode={expected} ")) {
                    return Err(format!("顯示器 {id} 未確認為 {expected}：{}", mode.trim()));
                }
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("找不到唯一的新 scrcpy 顯示器（等待 15 秒）".into());
            }
            // 子行程建立不代表顯示器已註冊，稍後重試以等待手機完成。
            thread::sleep(Duration::from_millis(200));
        }
    }
}

fn warning(out: &mut dyn Write, reason: &str, enabled: bool) -> io::Result<()> {
    if enabled {
        writeln!(
            out,
            "警告：無法設定 DeX 自由視窗：{reason}；投影繼續，app 可能保留原有模式。預設全螢幕；可用 --freeform 或 freeform = true 啟用自由視窗，--no-freeform 可關閉。"
        )
    } else {
        writeln!(
            out,
            "警告：DeX 全螢幕還原失敗：{reason}；投影繼續，app 可能保留原有模式。"
        )
    }
}

impl CommandRunner for Projection<'_> {
    fn run(&self, program: &str, args: &[&str]) -> io::Result<CommandOutput> {
        self.runner.run(program, args)
    }
    fn spawn(&self, program: &str, args: &[&str]) -> io::Result<Box<dyn RunningCommand>> {
        self.runner.spawn(program, args)
    }
    fn run_timeout(
        &self,
        program: &str,
        args: &[&str],
        timeout: Duration,
    ) -> io::Result<TimedOutput> {
        self.runner.run_timeout(program, args, timeout)
    }
    fn run_projection(&self, args: &[&str], out: &mut dyn Write) -> io::Result<CommandOutput> {
        let before = match self.snapshot() {
            Ok(before) => before,
            Err(err) => {
                warning(out, &err, self.enabled)?;
                return self.runner.run_projection(args, out);
            }
        };
        let mut child = self.runner.spawn_projection(args, out)?;
        match self.configure(&before) {
            Ok(()) => self.report_mode(out)?,
            Err(err) => warning(out, &err, self.enabled)?,
        }
        loop {
            if let Some(output) = child.try_wait()? {
                return Ok(output);
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
    fn spawn_projection(
        &self,
        args: &[&str],
        out: &mut dyn Write,
    ) -> io::Result<Box<dyn RunningCommand>> {
        let before = self.snapshot();
        let child = self.runner.spawn_projection(args, out)?;
        match before.and_then(|before| self.configure(&before)) {
            Ok(()) => self.report_mode(out)?,
            Err(err) => warning(out, &err, self.enabled)?,
        }
        Ok(child)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Android 16 dumpsys display 的固定片段，省略無關的屬性。
    const ONE: &str = "mBaseDisplayInfo=DisplayInfo{\"scrcpy\", displayId 82, displayGroupId 0, type VIRTUAL, uniqueId \"virtual:com.android.shell,2000,scrcpy,0\", owner com.android.shell (uid 2000), removeMode 1}\n\
        mOverrideDisplayInfo=DisplayInfo{\"scrcpy\", displayId 82, displayGroupId 0, type VIRTUAL, uniqueId \"virtual:com.android.shell,2000,scrcpy,0\", owner com.android.shell (uid 2000), removeMode 1}";
    const TWO: &str = "mBaseDisplayInfo=DisplayInfo{\"scrcpy\", displayId 82, type VIRTUAL, uniqueId \"virtual:com.android.shell,2000,scrcpy,0\", owner com.android.shell (uid 2000)}\n\
        mBaseDisplayInfo=DisplayInfo{\"scrcpy\", displayId 93, type VIRTUAL, uniqueId \"virtual:com.android.shell,2000,scrcpy,1\", owner com.android.shell (uid 2000)}";
    #[test]
    fn base_and_override_are_one_display() {
        assert_eq!(new_display(&BTreeSet::new(), ONE), Ok(Some(82)));
    }
    #[test]
    fn no_match_waits_for_registration() {
        assert_eq!(new_display(&BTreeSet::new(), "mDisplayId=0"), Ok(None));
    }
    #[test]
    fn multiple_new_displays_are_rejected() {
        assert_eq!(
            new_display(&BTreeSet::new(), TWO),
            Err("多個新的 scrcpy 顯示器，無法安全選擇".into())
        );
    }
    #[test]
    fn existing_session_is_excluded() {
        assert_eq!(new_display(&BTreeSet::from([82]), TWO), Ok(Some(93)));
    }
    #[test]
    fn another_owner_is_not_selected() {
        assert!(displays("DisplayInfo{displayId 82, type VIRTUAL, uniqueId \"virtual:com.android.shell,2000,scrcpy,0\", owner other (uid 1234)}").is_empty());
    }
    #[test]
    fn freeform_command_targets_selected_serial_and_display() {
        assert_eq!(
            mode_args("phone:5555", "82", "5"),
            [
                "-s",
                "phone:5555",
                "shell",
                "wm",
                "set-display-windowing-mode",
                "-d",
                "82",
                "5"
            ]
        );
    }
    struct Fake {
        replies: std::cell::RefCell<std::collections::VecDeque<Result<String, String>>>,
        calls: std::cell::RefCell<Vec<String>>,
    }
    impl Fake {
        fn new(replies: Vec<Result<String, String>>) -> Self {
            Self {
                replies: std::cell::RefCell::new(replies.into()),
                calls: std::cell::RefCell::new(Vec::new()),
            }
        }
    }
    struct Finished;
    impl RunningCommand for Finished {
        fn try_wait(&mut self) -> io::Result<Option<CommandOutput>> {
            Ok(Some(CommandOutput {
                code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
            }))
        }
        fn kill(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl CommandRunner for Fake {
        fn run(&self, program: &str, args: &[&str]) -> io::Result<CommandOutput> {
            self.calls
                .borrow_mut()
                .push(format!("{program} {}", args.join(" ")));
            Ok(CommandOutput {
                code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
            })
        }
        fn spawn(&self, program: &str, args: &[&str]) -> io::Result<Box<dyn RunningCommand>> {
            self.calls
                .borrow_mut()
                .push(format!("{program} {}", args.join(" ")));
            Ok(Box::new(Finished))
        }
        fn run_timeout(
            &self,
            program: &str,
            args: &[&str],
            _: Duration,
        ) -> io::Result<TimedOutput> {
            self.calls
                .borrow_mut()
                .push(format!("{program} {}", args.join(" ")));
            match self
                .replies
                .borrow_mut()
                .pop_front()
                .expect("unexpected adb call")
            {
                Ok(stdout) => Ok(TimedOutput::Exited(CommandOutput {
                    code: Some(0),
                    stdout,
                    stderr: String::new(),
                })),
                Err(stderr) => Ok(TimedOutput::Exited(CommandOutput {
                    code: Some(1),
                    stdout: String::new(),
                    stderr,
                })),
            }
        }
    }
    #[test]
    fn usb_setup_failure_warns_and_preserves_projection_success() {
        let runner = Fake::new(vec![
            Ok(String::new()),
            Ok(ONE.into()),
            Err("unsupported command".into()),
        ]);
        let mut out = Vec::new();
        let result = Projection::new(&runner, "phone", true)
            .run_projection(&["--new-display=1920x1080/160"], &mut out)
            .unwrap();
        assert!(result.success());
        assert!(
            String::from_utf8(out)
                .unwrap()
                .contains("unsupported command；投影繼續")
        );
    }
    #[test]
    fn wifi_setup_failure_keeps_child_alive() {
        let runner = Fake::new(vec![
            Ok(String::new()),
            Ok(ONE.into()),
            Err("unsupported command".into()),
        ]);
        let mut out = Vec::new();
        let mut child = Projection::new(&runner, "phone", true)
            .spawn_projection(&["--new-display=1920x1080/160"], &mut out)
            .unwrap();
        assert!(child.try_wait().unwrap().unwrap().success());
        assert!(String::from_utf8(out).unwrap().contains("投影繼續"));
    }
    #[test]
    fn disabled_feature_resets_a_persisted_display_mode_to_fullscreen() {
        let runner = Fake::new(vec![
            Ok(String::new()),
            Ok(ONE.into()),
            Ok(String::new()),
            Ok("display windowing mode=fullscreen for displayId=82".into()),
        ]);
        Projection::new(&runner, "phone", false)
            .run_projection(&["--new-display=1920x1080/160"], &mut Vec::new())
            .unwrap();
        assert_eq!(
            *runner.calls.borrow(),
            [
                "adb -s phone shell dumpsys display",
                "scrcpy --new-display=1920x1080/160",
                "adb -s phone shell dumpsys display",
                "adb -s phone shell wm set-display-windowing-mode -d 82 1",
                "adb -s phone shell wm get-display-windowing-mode -d 82"
            ]
        );
    }
    #[test]
    fn setup_uses_discovered_id_and_verifies_mode() {
        let runner = Fake::new(vec![
            Ok(String::new()),
            Ok(ONE.into()),
            Ok(String::new()),
            Ok("display windowing mode=freeform for displayId=82".into()),
        ]);
        Projection::new(&runner, "phone", true)
            .spawn_projection(&["--new-display=1920x1080/160"], &mut Vec::new())
            .unwrap();
        assert_eq!(
            *runner.calls.borrow(),
            [
                "adb -s phone shell dumpsys display",
                "scrcpy --new-display=1920x1080/160",
                "adb -s phone shell dumpsys display",
                "adb -s phone shell wm set-display-windowing-mode -d 82 5",
                "adb -s phone shell wm get-display-windowing-mode -d 82"
            ]
        );
    }
    #[test]
    fn ambiguous_displays_never_receive_a_mode_command() {
        let runner = Fake::new(vec![Ok(String::new()), Ok(TWO.into())]);
        Projection::new(&runner, "phone", true)
            .spawn_projection(&[], &mut Vec::new())
            .unwrap();
        assert_eq!(
            *runner.calls.borrow(),
            [
                "adb -s phone shell dumpsys display",
                "scrcpy ",
                "adb -s phone shell dumpsys display"
            ]
        );
    }
    #[test]
    fn disabled_mode_warning_reports_reset_failure_without_disable_advice() {
        let runner = Fake::new(vec![
            Ok(String::new()),
            Ok(ONE.into()),
            Err("unsupported command".into()),
        ]);
        let mut out = Vec::new();
        Projection::new(&runner, "phone", false)
            .spawn_projection(&[], &mut out)
            .unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "警告：DeX 全螢幕還原失敗：adb exit Some(1):  unsupported command；投影繼續，app 可能保留原有模式。\n"
        );
    }
}
