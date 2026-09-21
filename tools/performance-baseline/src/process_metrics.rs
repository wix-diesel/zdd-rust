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
        let pid = std::process::id().to_string();
        return command_metrics("ps", &["-o", "time=,rss=", "-p", &pid]);
    }
    #[allow(unreachable_code)]
    ProcessMetrics::default()
}

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
