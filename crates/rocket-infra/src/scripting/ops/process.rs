use deno_core::op2;
use std::collections::HashMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use crate::scripting::ops::ScriptOpError;

/// Runs `command` with `args`, draining stdout/stderr on background threads
/// while polling for exit so a child that fills its OS pipe buffer before
/// exiting can never deadlock this call. Kills the child (not any
/// grandchildren it spawned) if `timeout_ms` elapses first.
fn exec_impl(
    command: &str,
    args: &[String],
    cwd: Option<&str>,
    env: &HashMap<String, String>,
    timeout_ms: u64,
) -> Result<(String, String, i32), ScriptOpError> {
    let mut cmd = Command::new(command);
    cmd.args(args);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }

    let mut child = cmd.spawn().map_err(|e| ScriptOpError(format!("{command}: {e}")))?;

    let mut stdout_pipe = child
        .stdout
        .take()
        .ok_or_else(|| ScriptOpError(format!("{command}: missing stdout pipe")))?;
    let mut stderr_pipe = child
        .stderr
        .take()
        .ok_or_else(|| ScriptOpError(format!("{command}: missing stderr pipe")))?;

    let (stdout_tx, stdout_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = String::new();
        let _ = stdout_pipe.read_to_string(&mut buf);
        let _ = stdout_tx.send(buf);
    });
    let (stderr_tx, stderr_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = String::new();
        let _ = stderr_pipe.read_to_string(&mut buf);
        let _ = stderr_tx.send(buf);
    });

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ScriptOpError(format!("{command}: timed out after {timeout_ms}ms")));
                }
                thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(ScriptOpError(format!("{command}: {e}"))),
        }
    };

    let stdout = stdout_rx
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .unwrap_or_default();
    let stderr = stderr_rx
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .unwrap_or_default();
    let exit_code = status.code().unwrap_or(-1);
    Ok((stdout, stderr, exit_code))
}

#[op2]
#[string]
pub fn op_process_exec(
    #[string] command: String,
    #[string] args_json: String,
    #[string] cwd: String,
    #[string] env_json: String,
    timeout_ms: u32,
) -> Result<String, ScriptOpError> {
    let args: Vec<String> = serde_json::from_str(&args_json).unwrap_or_default();
    let env: HashMap<String, String> = serde_json::from_str(&env_json).unwrap_or_default();
    let cwd_opt = if cwd.is_empty() { None } else { Some(cwd.as_str()) };
    let (stdout, stderr, exit_code) = exec_impl(&command, &args, cwd_opt, &env, timeout_ms as u64)?;
    Ok(serde_json::to_string(&serde_json::json!({
        "stdout": stdout,
        "stderr": stderr,
        "exitCode": exit_code,
    }))
    .unwrap_or_else(|_| "{}".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn exec_runs_a_command_and_captures_stdout() {
        let (stdout, _stderr, exit_code) =
            exec_impl("echo", &["hello".to_string()], None, &HashMap::new(), 5000).expect("exec");
        assert_eq!(stdout.trim(), "hello");
        assert_eq!(exit_code, 0);
    }

    #[cfg(windows)]
    #[test]
    fn exec_runs_a_command_and_captures_stdout() {
        let (stdout, _stderr, exit_code) = exec_impl(
            "cmd",
            &["/C".to_string(), "echo".to_string(), "hello".to_string()],
            None,
            &HashMap::new(),
            5000,
        )
        .expect("exec");
        assert_eq!(stdout.trim(), "hello");
        assert_eq!(exit_code, 0);
    }

    #[cfg(unix)]
    #[test]
    fn exec_captures_non_zero_exit_code() {
        let (_stdout, _stderr, exit_code) = exec_impl(
            "sh",
            &["-c".to_string(), "exit 7".to_string()],
            None,
            &HashMap::new(),
            5000,
        )
        .expect("exec");
        assert_eq!(exit_code, 7);
    }

    #[cfg(windows)]
    #[test]
    fn exec_captures_non_zero_exit_code() {
        let (_stdout, _stderr, exit_code) = exec_impl(
            "cmd",
            &["/C".to_string(), "exit 7".to_string()],
            None,
            &HashMap::new(),
            5000,
        )
        .expect("exec");
        assert_eq!(exit_code, 7);
    }

    #[cfg(unix)]
    #[test]
    fn exec_times_out_and_returns_an_error() {
        let result = exec_impl("sleep", &["5".to_string()], None, &HashMap::new(), 200);
        assert!(result.is_err(), "expected a timeout error");
    }

    #[cfg(windows)]
    #[test]
    fn exec_times_out_and_returns_an_error() {
        let result = exec_impl(
            "cmd",
            &["/C".to_string(), "timeout".to_string(), "/T".to_string(), "5".to_string()],
            None,
            &HashMap::new(),
            200,
        );
        assert!(result.is_err(), "expected a timeout error");
    }

    #[cfg(unix)]
    #[test]
    fn exec_output_drain_is_bounded_by_the_timeout_even_with_a_lingering_grandchild() {
        // The direct child (`sh`) backgrounds a grandchild (`sleep 2`) that
        // inherits the stdout/stderr pipes and exits almost immediately
        // itself. Without the recv_timeout fix, the background reader
        // threads would block on read_to_string until the grandchild's pipe
        // handle closes ~2s later, well past the 500ms timeout given here.
        let start = Instant::now();
        let result = exec_impl(
            "sh",
            &["-c".to_string(), "(sleep 2 &) ; echo done".to_string()],
            None,
            &HashMap::new(),
            500,
        );
        let elapsed = start.elapsed();
        assert!(result.is_ok(), "expected the direct child's exit to succeed: {result:?}");
        assert!(
            elapsed < Duration::from_secs(1),
            "exec_impl should return promptly instead of blocking on the grandchild's held-open pipe, took {elapsed:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn exec_passes_custom_env_vars() {
        let mut env = HashMap::new();
        env.insert("ROCKET_TEST_VAR".to_string(), "sandbox-value".to_string());
        let (stdout, _stderr, _exit_code) = exec_impl(
            "sh",
            &["-c".to_string(), "echo $ROCKET_TEST_VAR".to_string()],
            None,
            &env,
            5000,
        )
        .expect("exec");
        assert_eq!(stdout.trim(), "sandbox-value");
    }

    #[cfg(windows)]
    #[test]
    fn exec_passes_custom_env_vars() {
        let mut env = HashMap::new();
        env.insert("ROCKET_TEST_VAR".to_string(), "sandbox-value".to_string());
        let (stdout, _stderr, _exit_code) = exec_impl(
            "cmd",
            &["/C".to_string(), "echo %ROCKET_TEST_VAR%".to_string()],
            None,
            &env,
            5000,
        )
        .expect("exec");
        assert_eq!(stdout.trim(), "sandbox-value");
    }

    #[cfg(unix)]
    #[test]
    fn exec_respects_cwd_option() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        std::fs::write(dir.path().join("marker.txt"), "x").expect("write marker");
        let cwd = dir.path().to_string_lossy().to_string();
        let (stdout, _stderr, _exit_code) =
            exec_impl("ls", &[], Some(&cwd), &HashMap::new(), 5000).expect("exec");
        assert!(stdout.contains("marker.txt"));
    }

    #[cfg(windows)]
    #[test]
    fn exec_respects_cwd_option() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        std::fs::write(dir.path().join("marker.txt"), "x").expect("write marker");
        let cwd = dir.path().to_string_lossy().to_string();
        let (stdout, _stderr, _exit_code) = exec_impl(
            "cmd",
            &["/C".to_string(), "dir".to_string(), "/B".to_string()],
            Some(&cwd),
            &HashMap::new(),
            5000,
        )
        .expect("exec");
        assert!(stdout.contains("marker.txt"));
    }
}
