use std::io;
use std::process::Command;

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

/// 所有 adb / scrcpy 呼叫都經過這裡，測試才能換成回傳寫死輸出的假實作，不需要實機與工具。
pub trait CommandRunner {
    /// 執行 `program args...` 並等它結束（會阻塞）。
    ///
    /// # Errors
    /// 程式不存在時回傳 `ErrorKind::NotFound`；其他無法啟動的情況（權限等）回傳對應的 `io::Error`。
    /// 程式有執行但以非 0 結束不算錯誤，由呼叫端看 `code` 判斷。
    fn run(&self, program: &str, args: &[&str]) -> io::Result<CommandOutput>;
}

/// 真正啟動子行程的實作。
pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[&str]) -> io::Result<CommandOutput> {
        let output = Command::new(program).args(args).output()?;
        Ok(CommandOutput {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}
