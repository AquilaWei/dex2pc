use std::io::{self, Read};
use std::process::{Child, Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// 外部指令執行完的結果；stdout/stderr 以 UTF-8（有損）解碼。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    /// 被訊號中止時為 `None`。
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

/// 可輪詢的子行程；結束時取回輸出，強制停止時必須回收行程。
pub trait RunningCommand {
    /// 尚未結束時為 None；查詢或讀取輸出失敗時回傳 I/O 錯誤。
    fn try_wait(&mut self) -> io::Result<Option<CommandOutput>>;
    /// 停止並回收子行程；作業系統無法停止時回傳 I/O 錯誤。
    fn kill(&mut self) -> io::Result<()>;
}

/// 逾時與被訊號中止不同，前者可重試，後者應停止工作階段。
#[derive(Debug, PartialEq, Eq)]
pub enum TimedOutput {
    Exited(CommandOutput),
    TimedOut,
}

/// 所有 adb / scrcpy 呼叫都經過這裡，測試才能換成回傳寫死輸出的假實作，不需要實機與工具。
pub trait CommandRunner {
    /// 執行 `program args...` 並等它結束（會阻塞）。
    ///
    /// # Errors
    /// 程式不存在時回傳 `ErrorKind::NotFound`；其他無法啟動的情況（權限等）回傳對應的 `io::Error`。
    /// 程式有執行但以非 0 結束不算錯誤，由呼叫端看 `code` 判斷。
    fn run(&self, program: &str, args: &[&str]) -> io::Result<CommandOutput>;

    /// 啟動可輪詢的行程，避免 Wi-Fi 斷線後永遠阻塞；無法啟動時回傳 I/O 錯誤。
    fn spawn(&self, _program: &str, _args: &[&str]) -> io::Result<Box<dyn RunningCommand>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "不支援子行程監測",
        ))
    }

    /// 有限時間的連線探測；逾時必須停止並回收行程，啟動失敗回傳 I/O 錯誤。
    fn run_timeout(
        &self,
        _program: &str,
        _args: &[&str],
        _timeout: Duration,
    ) -> io::Result<TimedOutput> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "不支援指令逾時"))
    }

    /// 配對碼必須由 adb 直接從終端讀取；正式實作繼承 stdin/stdout/stderr。
    /// 無法啟動時回傳 I/O 錯誤，被訊號中止時回傳 None；預設供假的 runner 使用。
    fn interactive(&self, program: &str, args: &[&str]) -> io::Result<Option<i32>> {
        Ok(self.run(program, args)?.code)
    }
}

/// 真正啟動子行程的實作。
#[derive(Clone, Copy)]
pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn spawn(&self, program: &str, args: &[&str]) -> io::Result<Box<dyn RunningCommand>> {
        let mut child = Command::new(program)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        // 持續讀取兩條管線，否則 scrcpy 大量輸出可能塞滿 pipe，讓子行程無法結束。
        let stdout = drain(child.stdout.take().expect("stdout 已設為 pipe"));
        let stderr = drain(child.stderr.take().expect("stderr 已設為 pipe"));
        Ok(Box::new(SystemChild {
            child,
            stdout: Some(stdout),
            stderr: Some(stderr),
        }))
    }

    fn run_timeout(
        &self,
        program: &str,
        args: &[&str],
        timeout: Duration,
    ) -> io::Result<TimedOutput> {
        let mut child = self.spawn(program, args)?;
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(output) = child.try_wait()? {
                return Ok(TimedOutput::Exited(output));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                child.kill()?;
                return Ok(TimedOutput::TimedOut);
            }
            thread::sleep(remaining.min(Duration::from_millis(10)));
        }
    }

    fn interactive(&self, program: &str, args: &[&str]) -> io::Result<Option<i32>> {
        Ok(Command::new(program).args(args).status()?.code())
    }

    fn run(&self, program: &str, args: &[&str]) -> io::Result<CommandOutput> {
        let output = Command::new(program).args(args).output()?;
        Ok(CommandOutput {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

fn drain(mut pipe: impl Read + Send + 'static) -> JoinHandle<io::Result<String>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    })
}

fn collected(reader: &mut Option<JoinHandle<io::Result<String>>>) -> io::Result<String> {
    reader
        .take()
        .expect("輸出只讀一次")
        .join()
        .map_err(|_| io::Error::other("讀取子行程輸出的執行緒中止"))?
}

struct SystemChild {
    child: Child,
    stdout: Option<JoinHandle<io::Result<String>>>,
    stderr: Option<JoinHandle<io::Result<String>>>,
}

impl RunningCommand for SystemChild {
    fn try_wait(&mut self) -> io::Result<Option<CommandOutput>> {
        let Some(status) = self.child.try_wait()? else {
            return Ok(None);
        };
        Ok(Some(CommandOutput {
            code: status.code(),
            stdout: collected(&mut self.stdout)?,
            stderr: collected(&mut self.stderr)?,
        }))
    }

    fn kill(&mut self) -> io::Result<()> {
        if self.child.try_wait()?.is_none() {
            self.child.kill()?;
        }
        self.child.wait()?;
        Ok(())
    }
}

impl Drop for SystemChild {
    fn drop(&mut self) {
        // 錯誤或中斷提前返回也要回收子行程，避免遺留背景視窗。
        let _ = self.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_command_preserves_exit_code_and_stderr() {
        assert_eq!(
            SystemRunner
                .run_timeout(
                    "sh",
                    &["-c", "printf reason >&2; exit 2"],
                    Duration::from_secs(5)
                )
                .unwrap(),
            TimedOutput::Exited(CommandOutput {
                code: Some(2),
                stdout: String::new(),
                stderr: "reason".into()
            })
        );
    }

    #[test]
    fn bounded_command_kills_sleeping_child_on_timeout() {
        assert_eq!(
            SystemRunner
                .run_timeout("sh", &["-c", "exec sleep 30"], Duration::ZERO)
                .unwrap(),
            TimedOutput::TimedOut
        );
    }

    #[test]
    fn child_stderr_larger_than_pipe_capacity_is_drained() {
        let result = SystemRunner
            .run_timeout(
                "sh",
                &["-c", "head -c 1048576 /dev/zero >&2"],
                Duration::from_secs(5),
            )
            .unwrap();
        let TimedOutput::Exited(output) = result else {
            panic!("stderr 不應塞滿管線");
        };
        assert_eq!(output.code, Some(0));
        assert_eq!(output.stderr.len(), 1_048_576);
    }
}
