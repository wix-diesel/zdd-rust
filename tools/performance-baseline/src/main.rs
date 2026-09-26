use std::env;
use std::error::Error;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

mod allocations;
mod datasets;
mod process_metrics;
mod workload;

use workload::{Retention, WorkloadResult};

#[global_allocator]
static ALLOCATOR: allocations::CountingAllocator = allocations::CountingAllocator;

fn main() -> Result<(), Box<dyn Error>> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments
        .first()
        .is_some_and(|argument| argument == "--child")
    {
        return child(&arguments[1..]);
    }
    orchestrate(&arguments)
}

fn child(arguments: &[String]) -> Result<(), Box<dyn Error>> {
    if arguments.len() != 4 {
        return Err("child requires CASE ORDER RETENTION ITERATIONS".into());
    }
    let retention = Retention::parse(&arguments[2]).ok_or("retention must be keep or drop")?;
    let iterations = arguments[3].parse()?;
    let result = if arguments[0] == "family-powerset" {
        WorkloadResult::family(retention, iterations)?
    } else {
        let datasets = datasets::graph_datasets()?;
        let dataset = datasets
            .iter()
            .find(|dataset| dataset.name == arguments[0])
            .ok_or("unknown graph case")?;
        let order = match arguments[1].as_str() {
            "good" => dataset.good_order.clone(),
            "bad" => dataset.bad_order.clone(),
            _ => return Err("order must be good or bad".into()),
        };
        WorkloadResult::graph(dataset, &arguments[1], order, retention, iterations)?
    };
    println!("{}", result.json());
    Ok(())
}

fn orchestrate(arguments: &[String]) -> Result<(), Box<dyn Error>> {
    let options = Options::parse(arguments)?;
    let mut lines = vec![environment_json()];
    let executable = env::current_exe()?;
    let mut cases = datasets::graph_datasets()?
        .into_iter()
        .map(|dataset| dataset.name)
        .collect::<Vec<_>>();
    cases.push("family-powerset");
    if let Some(selected) = options.case.as_deref() {
        cases.retain(|case| *case == selected);
        if cases.is_empty() {
            return Err(format!("unknown case: {selected}").into());
        }
    }
    for case in cases {
        let orders: &[&str] = if case == "family-powerset" {
            &["fixed"]
        } else {
            &["good", "bad"]
        };
        for order in orders {
            for retention in ["keep", "drop"] {
                lines.push(run_child(
                    &executable,
                    case,
                    order,
                    retention,
                    options.iterations,
                    options.timeout,
                )?);
            }
        }
    }
    let output = format!("{}\n", lines.join("\n"));
    if let Some(path) = options.output {
        std::fs::write(path, output)?;
    } else {
        print!("{output}");
    }
    Ok(())
}

fn run_child(
    executable: &PathBuf,
    case: &str,
    order: &str,
    retention: &str,
    iterations: usize,
    timeout: Duration,
) -> Result<String, Box<dyn Error>> {
    let mut child = Command::new(executable)
        .args(["--child", case, order, retention, &iterations.to_string()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let mut stdout = String::new();
            let mut stderr = String::new();
            child.stdout.take().unwrap().read_to_string(&mut stdout)?;
            child.stderr.take().unwrap().read_to_string(&mut stderr)?;
            if status.success() {
                return Ok(stdout.trim().to_owned());
            }
            return Ok(failure_json(
                "failed-or-oom",
                case,
                order,
                retention,
                started.elapsed(),
                &format!("exit={status}; {stderr}"),
            ));
        }
        if started.elapsed() >= timeout {
            child.kill()?;
            child.wait()?;
            return Ok(failure_json(
                "timeout",
                case,
                order,
                retention,
                started.elapsed(),
                "workload exceeded its configured deadline",
            ));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn failure_json(
    status: &str,
    case: &str,
    order: &str,
    retention: &str,
    elapsed: Duration,
    detail: &str,
) -> String {
    format!(
        "{{\"status\":\"{status}\",\"case\":\"{case}\",\"order\":\"{order}\",\"retention\":\"{retention}\",\"elapsed_seconds\":{:.9},\"detail\":\"{}\"}}",
        elapsed.as_secs_f64(),
        json_escape(detail)
    )
}

fn environment_json() -> String {
    let commit = command_text("git", &["rev-parse", "HEAD"]);
    let rustc = command_text("rustc", &["-Vv"]);
    let cpu = cpu_description();
    let memory = memory_bytes();
    format!(
        "{{\"record_type\":\"environment\",\"commit\":\"{}\",\"rustc\":\"{}\",\"profile\":\"{}\",\"release_opt_level\":3,\"release_lto\":false,\"release_codegen_units\":16,\"panic_strategy\":\"unwind\",\"target_os\":\"{}\",\"target_arch\":\"{}\",\"cpu\":\"{}\",\"memory_bytes\":{},\"threads\":{},\"gc\":\"backend-managed\",\"shared_cache_entries\":262144,\"ordering_conversion_included\":false}}",
        json_escape(&commit),
        json_escape(&rustc),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        env::consts::OS,
        env::consts::ARCH,
        json_escape(&cpu),
        memory.map_or_else(|| "null".to_owned(), |value| value.to_string()),
        thread::available_parallelism().map_or(1, usize::from),
    )
}

fn command_text(program: &str, arguments: &[&str]) -> String {
    Command::new(program)
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .replace('\n', " | ")
        })
        .unwrap_or_else(|| "unavailable".to_owned())
}

fn cpu_description() -> String {
    #[cfg(target_os = "windows")]
    return command_text(
        "powershell",
        &[
            "-NoProfile",
            "-Command",
            "(Get-CimInstance Win32_Processor | Select-Object -First 1 -ExpandProperty Name)",
        ],
    );
    #[cfg(target_os = "linux")]
    return std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|contents| {
            contents
                .lines()
                .find_map(|line| line.strip_prefix("model name\t: ").map(str::to_owned))
        })
        .unwrap_or_else(|| "unavailable".to_owned());
    #[cfg(target_os = "macos")]
    return command_text("sysctl", &["-n", "machdep.cpu.brand_string"]);
    #[allow(unreachable_code)]
    "unavailable".to_owned()
}

fn memory_bytes() -> Option<u64> {
    #[cfg(target_os = "windows")]
    return command_text(
        "powershell",
        &[
            "-NoProfile",
            "-Command",
            "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
        ],
    )
    .parse()
    .ok();
    #[cfg(target_os = "linux")]
    return std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|contents| {
            contents.lines().find_map(|line| {
                line.strip_prefix("MemTotal:")
                    .and_then(|value| value.split_whitespace().next())
                    .and_then(|value| value.parse::<u64>().ok())
            })
        })
        .map(|kilobytes| kilobytes * 1_024);
    #[cfg(target_os = "macos")]
    return command_text("sysctl", &["-n", "hw.memsize"]).parse().ok();
    #[allow(unreachable_code)]
    None
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

struct Options {
    iterations: usize,
    timeout: Duration,
    output: Option<PathBuf>,
    case: Option<String>,
}

impl Options {
    fn parse(arguments: &[String]) -> Result<Self, Box<dyn Error>> {
        let mut options = Self {
            iterations: 100,
            timeout: Duration::from_secs(300),
            output: None,
            case: None,
        };
        let mut index = 0;
        while index < arguments.len() {
            let value = arguments.get(index + 1).ok_or("option requires a value")?;
            match arguments[index].as_str() {
                "--iterations" => options.iterations = value.parse()?,
                "--timeout-seconds" => options.timeout = Duration::from_secs(value.parse()?),
                "--output" => options.output = Some(value.into()),
                "--case" => options.case = Some(value.clone()),
                option => return Err(format!("unknown option: {option}").into()),
            }
            index += 2;
        }
        if options.iterations == 0 {
            return Err("iterations must be positive".into());
        }
        Ok(options)
    }
}
