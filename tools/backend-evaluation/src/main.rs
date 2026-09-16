mod custom;
mod oxidd_adapter;
mod workload;

use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let backend = args.next().unwrap_or_else(|| "all".to_owned());
    let variables = args.next().map(|v| v.parse()).transpose()?.unwrap_or(16);
    let rounds = args.next().map(|v| v.parse()).transpose()?.unwrap_or(100);
    if variables == 0 || variables > 63 {
        return Err("variables must be in 1..=63".into());
    }

    let sets = workload::path_matchings(variables);
    match backend.as_str() {
        "custom" => {
            let row = custom::run(variables, rounds, &sets)?;
            print_row(
                "custom",
                variables,
                rounds,
                sets.len(),
                row.build.as_secs_f64(),
                row.filters.as_secs_f64(),
                row.checksum,
                row.nodes_after_build,
                row.nodes_after_filters,
            );
        }
        "oxidd" => {
            let row = oxidd_adapter::run(variables, rounds, &sets)?;
            print_row(
                "oxidd",
                variables,
                rounds,
                sets.len(),
                row.build.as_secs_f64(),
                row.filters.as_secs_f64(),
                row.checksum,
                row.nodes_after_build,
                row.nodes_after_filters,
            );
        }
        "all" => {
            let custom = custom::run(variables, rounds, &sets)?;
            let oxidd = oxidd_adapter::run(variables, rounds, &sets)?;
            assert_eq!(custom.checksum, oxidd.checksum);
            print_row(
                "custom",
                variables,
                rounds,
                sets.len(),
                custom.build.as_secs_f64(),
                custom.filters.as_secs_f64(),
                custom.checksum,
                custom.nodes_after_build,
                custom.nodes_after_filters,
            );
            print_row(
                "oxidd",
                variables,
                rounds,
                sets.len(),
                oxidd.build.as_secs_f64(),
                oxidd.filters.as_secs_f64(),
                oxidd.checksum,
                oxidd.nodes_after_build,
                oxidd.nodes_after_filters,
            );
        }
        _ => return Err("backend must be custom, oxidd, or all".into()),
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn print_row(
    backend: &str,
    variables: u32,
    rounds: u32,
    solutions: usize,
    build_seconds: f64,
    filter_seconds: f64,
    checksum: u128,
    nodes_after_build: usize,
    nodes_after_filters: usize,
) {
    println!(
        "backend={backend} variables={variables} solutions={solutions} rounds={rounds} \
         build_seconds={build_seconds:.6} filter_seconds={filter_seconds:.6} \
         checksum={checksum} nodes_after_build={nodes_after_build} \
         nodes_after_filters={nodes_after_filters}"
    );
}
