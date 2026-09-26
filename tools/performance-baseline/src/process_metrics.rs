#[cfg(any(target_os = "linux", target_os = "windows"))]
use std::process::Command;

#[derive(Default)]
pub struct ProcessMetrics {
    pub cpu_seconds: Option<f64>,
    pub peak_rss_bytes: Option<u64>,
}

pub fn current() -> ProcessMetrics {
    #[cfg(target_os = "windows")]
    {
        let script = format!(
            "$p=Get-Process -Id {}; [Console]::Write(('{{0}};{{1}}' -f $p.CPU,$p.PeakWorkingSet64))",
            std::process::id()
        );
        return command_metrics("powershell", &["-NoProfile", "-Command", &script]);
    }
    #[cfg(target_os = "linux")]
    {
        return linux_metrics();
    }
    #[cfg(target_os = "macos")]
    {
        return macos_metrics();
    }
    #[allow(unreachable_code)]
    ProcessMetrics::default()
}

#[cfg(target_os = "windows")]
fn command_metrics(program: &str, arguments: &[&str]) -> ProcessMetrics {
    let Ok(output) = Command::new(program).args(arguments).output() else {
        return ProcessMetrics::default();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let values = text.trim().split(';').collect::<Vec<_>>();
    if values.len() == 2 {
        return ProcessMetrics {
            cpu_seconds: values[0].trim().replace(',', ".").parse().ok(),
            peak_rss_bytes: values[1].trim().parse().ok(),
        };
    }
    ProcessMetrics::default()
}

#[cfg(target_os = "macos")]
fn macos_metrics() -> ProcessMetrics {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: `usage` points to writable storage for one `rusage`. On success,
    // `getrusage` initializes the complete value before it is read below.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
        return ProcessMetrics::default();
    }
    // SAFETY: the successful `getrusage` call above initialized `usage`.
    let usage = unsafe { usage.assume_init() };
    ProcessMetrics {
        cpu_seconds: Some(
            timeval_seconds(usage.ru_utime) + timeval_seconds(usage.ru_stime),
        ),
        // Unlike Linux, macOS reports `ru_maxrss` in bytes.
        peak_rss_bytes: u64::try_from(usage.ru_maxrss).ok(),
    }
}

#[cfg(target_os = "macos")]
fn timeval_seconds(value: libc::timeval) -> f64 {
    value.tv_sec as f64 + value.tv_usec as f64 / 1_000_000.0
}

#[cfg(target_os = "linux")]
fn linux_metrics() -> ProcessMetrics {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let peak_rss_bytes = status.lines().find_map(|line| {
        line.strip_prefix("VmHWM:")
            .and_then(|value| value.split_whitespace().next())
            .and_then(|value| value.parse::<u64>().ok())
            .map(|kilobytes| kilobytes * 1_024)
    });
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let ticks = stat
        .split_whitespace()
        .skip(13)
        .take(2)
        .filter_map(|value| value.parse::<u64>().ok())
        .sum::<u64>();
    let ticks_per_second = Command::new("getconf")
        .arg("CLK_TCK")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|value| value.trim().parse::<u64>().ok());
    ProcessMetrics {
        cpu_seconds: ticks_per_second.map(|frequency| ticks as f64 / frequency as f64),
        peak_rss_bytes,
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn macos_reports_cpu_time_and_peak_rss() {
        let metrics = current();
        assert!(metrics.cpu_seconds.is_some());
        assert!(metrics.peak_rss_bytes.is_some_and(|bytes| bytes > 0));
    }
}
