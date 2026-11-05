use std::collections::BTreeSet;
use std::io::{self, Write};
use std::thread;
use std::time::{Duration, Instant};

use crate::runner::{CommandOutput, CommandRunner, RunningCommand, TimedOutput};

/// Only shell-owned virtual scrcpy displays are eligible. Deduplicate base/override records.
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

fn mode_args(serial: &str, id: &str) -> Vec<String> {
    [
        "-s",
        serial,
        "shell",
        "wm",
        "set-display-windowing-mode",
        "-d",
        id,
        "5",
    ]
    .map(str::to_owned)
    .to_vec()
}

/// Decorates both launch paths so every reconnect takes a fresh display snapshot.
/// Failures only warn: freeform must never prevent a usable projection.
pub struct Projection<'a> {
    runner: &'a dyn CommandRunner,
    serial: &'a str,
    enabled: bool,
}
impl<'a> Projection<'a> {
    pub fn new(runner: &'a dyn CommandRunner, serial: &'a str, enabled: bool) -> Self {
        Self {
            runner,
            serial,
            enabled,
        }
    }

    fn adb(&self, args: &[&str]) -> Result<String, String> {
        match self.runner.run_timeout("adb", args, Duration::from_secs(2)) {
            Ok(TimedOutput::Exited(output)) if output.success() => Ok(output.stdout),
            Ok(TimedOutput::Exited(output)) => Err(format!(
                "adb exit {:?}: {} {}",
                output.code,
                output.stdout.trim(),
                output.stderr.trim()
            )),
            Ok(TimedOutput::TimedOut) => Err("adb 逾時（2 秒）".into()),
            Err(err) => Err(err.to_string()),
        }
    }

    fn snapshot(&self) -> Result<BTreeSet<u32>, String> {
        self.adb(&["-s", self.serial, "shell", "dumpsys", "display"])
            .map(|s| displays(&s))
    }

    fn configure(&self, before: &BTreeSet<u32>) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let dump = self.adb(&["-s", self.serial, "shell", "dumpsys", "display"])?;
            if let Some(id) = new_display(before, &dump)? {
                let args = mode_args(self.serial, &id.to_string());
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
                if !mode.contains("display windowing mode=freeform") {
                    return Err(format!("顯示器 {id} 未確認為 freeform：{}", mode.trim()));
                }
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("找不到唯一的新 scrcpy 顯示器（等待 8 秒）".into());
            }
            // Display registration happens after scrcpy starts, not when its process is spawned.
            thread::sleep(Duration::from_millis(200));
        }
    }
}

fn warning(out: &mut dyn Write, reason: &str) -> io::Result<()> {
    writeln!(
        out,
        "警告：無法啟用 DeX 自由視窗：{reason}；投影繼續，app 可能使用全螢幕。可用 --no-freeform 或 freeform = false 關閉。"
    )
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
        if !self.enabled {
            return self.runner.run_projection(args, out);
        }
        let before = match self.snapshot() {
            Ok(before) => before,
            Err(err) => {
                warning(out, &err)?;
                return self.runner.run_projection(args, out);
            }
        };
        let mut child = self.runner.spawn_projection(args, out)?;
        match self.configure(&before) {
            Ok(()) => writeln!(out, "DeX 自由視窗已啟用")?,
            Err(err) => warning(out, &err)?,
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
        if !self.enabled {
            return self.runner.spawn_projection(args, out);
        }
        let before = self.snapshot();
        let child = self.runner.spawn_projection(args, out)?;
        match before.and_then(|before| self.configure(&before)) {
            Ok(()) => writeln!(out, "DeX 自由視窗已啟用")?,
            Err(err) => warning(out, &err)?,
        }
        Ok(child)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Fixed excerpts of Android 16 dumpsys display, with unrelated properties omitted.
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
            mode_args("phone:5555", "82"),
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
    fn disabled_feature_never_calls_adb() {
        let runner = Fake::new(vec![]);
        Projection::new(&runner, "phone", false)
            .run_projection(&["--new-display=1920x1080/160"], &mut Vec::new())
            .unwrap();
        assert_eq!(
            *runner.calls.borrow(),
            ["scrcpy --new-display=1920x1080/160"]
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
}
