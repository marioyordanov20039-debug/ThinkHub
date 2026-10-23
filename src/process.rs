//! Bounded subprocess transport for optional, fixed system-tool operations.
use anyhow::{ensure, Context, Result};
use std::{
    io::Read,
    path::Path,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

pub fn run(
    program: &Path,
    arguments: &[&str],
    timeout: Duration,
    max_bytes: usize,
) -> Result<Output> {
    let mut child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Could not start optional system tool")?;
    let drain = |pipe: Box<dyn Read + Send>| {
        thread::spawn(move || {
            let mut data = vec![];
            let result = pipe.take(max_bytes as u64 + 1).read_to_end(&mut data);
            (result, data)
        })
    };
    let stdout = drain(Box::new(child.stdout.take().context("Missing stdout")?));
    let stderr = drain(Box::new(child.stderr.take().context("Missing stderr")?));
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let (out_result, out) = stdout
        .join()
        .map_err(|_| anyhow::anyhow!("Output reader failed"))?;
    let (err_result, err) = stderr
        .join()
        .map_err(|_| anyhow::anyhow!("Error reader failed"))?;
    out_result?;
    err_result?;
    let status = status.context("Optional system tool timed out")?;
    ensure!(
        out.len() <= max_bytes && err.len() <= max_bytes,
        "Optional system tool response exceeded size limit"
    );
    Ok(Output {
        status,
        stdout: out,
        stderr: err,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excessive_output_and_timeouts_fail_cleanly() {
        let error = run(
            Path::new("/usr/bin/printf"),
            &["0123456789"],
            Duration::from_secs(1),
            4,
        )
        .unwrap_err();
        assert!(error.to_string().contains("size limit"));
        let error = run(
            Path::new("/usr/bin/sleep"),
            &["2"],
            Duration::from_millis(20),
            100,
        )
        .unwrap_err();
        assert!(error.to_string().contains("timed out"));
    }
}
