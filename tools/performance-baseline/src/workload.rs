use std::error::Error;
use std::fmt::Write;
use std::time::{Duration, Instant};

use zdd_family::{BuildStats, EdgeOrder, FamilySpace, GraphSpace, QueryLimits, SpaceStats};

use crate::allocations;
use crate::datasets::GraphDataset;
use crate::process_metrics;

#[derive(Clone, Copy)]
pub enum Retention {
    Keep,
    Drop,
}

impl Retention {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "keep" => Some(Self::Keep),
            "drop" => Some(Self::Drop),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Keep => "keep",
            Self::Drop => "drop",
        }
    }
}

pub struct WorkloadResult {
    case: String,
    order: String,
    retention: Retention,
    iterations: usize,
    wall: Duration,
    compaction: Duration,
    allocation_calls: usize,
    allocated_bytes: usize,
    process: process_metrics::ProcessMetrics,
    space: SpaceStats,
    build: BuildStats,
    reachable_nodes: usize,
    count_bits: usize,
    index_bytes: usize,
    extracted_sets: usize,
}

impl WorkloadResult {
    pub fn graph(
        dataset: &GraphDataset,
        order_name: &str,
        order: EdgeOrder,
        retention: Retention,
        iterations: usize,
    ) -> Result<Self, Box<dyn Error>> {
        allocations::reset();
        let started = Instant::now();
        let space = GraphSpace::builder(&dataset.graph)
            .ordering(order)
            .build()?;
        let report = space.matchings_with_stats()?;
        let base = report.value;
        let mut retained = Vec::with_capacity(match retention {
            Retention::Keep => iterations,
            Retention::Drop => 1,
        });
        let mut extracted_sets = 0usize;
        let mut reachable_nodes = 0usize;
        let mut count_bits = 0usize;
        let mut index_bytes = 0usize;

        for iteration in 0..iterations {
            let edge = dataset
                .graph
                .edge_id(iteration % dataset.graph.edge_count())?;
            let filtered = if iteration % 2 == 0 {
                base.filter_contains(edge)?
            } else {
                base.filter_excludes(edge)?
            };
            let maximum = (dataset.graph.vertex_count() / 2).max(1);
            let filtered = filtered.cardinality().at_most(iteration % (maximum + 1))?;
            let index = filtered.count_index(&QueryLimits::default())?;
            reachable_nodes = reachable_nodes.max(index.stats().snapshot_nodes);
            count_bits = count_bits.max(index.stats().max_count_bits);
            index_bytes = index_bytes.max(
                index.stats().snapshot_nodes * std::mem::size_of::<usize>() * 4
                    + index.stats().total_count_bits.div_ceil(8),
            );
            extracted_sets += filtered.iter().take(8).count();
            match retention {
                Retention::Keep => retained.push(filtered),
                Retention::Drop => {
                    retained.clear();
                    retained.push(filtered);
                }
            }
        }

        let compact_started = Instant::now();
        let (compacted, compacted_roots) = space.compact(&retained)?;
        let compaction = compact_started.elapsed();
        std::hint::black_box((&compacted, &compacted_roots));
        let wall = started.elapsed();
        let space_stats = space.stats();
        let (allocation_calls, allocated_bytes) = allocations::snapshot();
        let process = process_metrics::current();
        Ok(Self {
            case: dataset.name.to_owned(),
            order: order_name.to_owned(),
            retention,
            iterations,
            wall,
            compaction,
            allocation_calls,
            allocated_bytes,
            process,
            space: space_stats,
            build: report.stats,
            reachable_nodes,
            count_bits,
            index_bytes,
            extracted_sets,
        })
    }

    pub fn family(retention: Retention, iterations: usize) -> Result<Self, Box<dyn Error>> {
        allocations::reset();
        let started = Instant::now();
        let space = FamilySpace::new(32)?;
        let base = space.powerset()?.cardinality().between(8..=24)?;
        let mut retained = Vec::new();
        let mut extracted_sets = 0usize;
        let mut reachable_nodes = 0usize;
        let mut count_bits = 0usize;
        let mut index_bytes = 0usize;
        for iteration in 0..iterations {
            let variable = space.variable(iteration % 32)?;
            let filtered = if iteration % 2 == 0 {
                base.filter_contains(variable)?
            } else {
                base.filter_excludes(variable)?
            };
            let filtered = filtered.cardinality().between(8..=(8 + iteration % 17))?;
            let index = filtered.count_index(&QueryLimits::default())?;
            reachable_nodes = reachable_nodes.max(index.stats().snapshot_nodes);
            count_bits = count_bits.max(index.stats().max_count_bits);
            index_bytes = index_bytes.max(
                index.stats().snapshot_nodes * std::mem::size_of::<usize>() * 4
                    + index.stats().total_count_bits.div_ceil(8),
            );
            extracted_sets += filtered.iter().take(8).count();
            match retention {
                Retention::Keep => retained.push(filtered),
                Retention::Drop => {
                    retained.clear();
                    retained.push(filtered);
                }
            }
        }
        let compact_started = Instant::now();
        let (compacted, compacted_roots) = space.compact(&retained)?;
        let compaction = compact_started.elapsed();
        std::hint::black_box((&compacted, &compacted_roots));
        let wall = started.elapsed();
        let space_stats = space.stats();
        let (allocation_calls, allocated_bytes) = allocations::snapshot();
        let process = process_metrics::current();
        Ok(Self {
            case: "family-powerset".to_owned(),
            order: "fixed".to_owned(),
            retention,
            iterations,
            wall,
            compaction,
            allocation_calls,
            allocated_bytes,
            process,
            space: space_stats,
            build: BuildStats::default(),
            reachable_nodes,
            count_bits,
            index_bytes,
            extracted_sets,
        })
    }

    pub fn json(&self) -> String {
        let mut output = String::new();
        write!(
            output,
            "{{\"status\":\"ok\",\"case\":\"{}\",\"order\":\"{}\",\"retention\":\"{}\",\"iterations\":{},\"wall_seconds\":{:.9},",
            self.case,
            self.order,
            self.retention.name(),
            self.iterations,
            self.wall.as_secs_f64()
        )
        .unwrap();
        optional_float(&mut output, "cpu_seconds", self.process.cpu_seconds);
        optional_u64(&mut output, "peak_rss_bytes", self.process.peak_rss_bytes);
        write!(
            output,
            "\"allocation_calls\":{},\"allocated_bytes\":{},\"live_nodes\":{},\"peak_live_nodes\":{},\"nodes_created\":{},\"reachable_nodes\":{},\"frontier_layers\":{},\"frontier_current_states\":{},\"frontier_peak_states\":{},\"frontier_transitions\":{},\"frontier_rejected\":{},\"frontier_merged\":{},\"transition_tape_entries\":{},\"operation_memo_peak\":{},\"operation_memo_hits\":{},\"cache_entries\":{},\"cache_hits\":{},\"cache_misses\":{},\"cache_evictions\":{},\"count_bit_length\":{},\"count_index_estimated_bytes\":{},\"extracted_sets\":{},\"compaction_seconds\":{:.9},\"conversion_seconds\":0.0}}",
            self.allocation_calls,
            self.allocated_bytes,
            self.space.live_nodes,
            self.space.peak_live_nodes,
            self.space.nodes_created,
            self.reachable_nodes,
            self.build.layers_processed,
            self.build.current_frontier_states,
            self.build.peak_frontier_states,
            self.build.transitions_attempted,
            self.build.branches_rejected,
            self.build.states_merged,
            self.build.transition_tape_entries,
            self.build.operation.peak_operation_memo_entries,
            self.build.operation.operation_memo_hits,
            self.space.shared_cache_entries,
            self.space.shared_cache_hits,
            self.space.shared_cache_misses,
            self.space.shared_cache_evictions,
            self.count_bits,
            self.index_bytes,
            self.extracted_sets,
            self.compaction.as_secs_f64(),
        )
        .unwrap();
        output
    }
}

fn optional_float(output: &mut String, name: &str, value: Option<f64>) {
    match value {
        Some(value) => write!(output, "\"{name}\":{value:.9},").unwrap(),
        None => write!(output, "\"{name}\":null,").unwrap(),
    }
}

fn optional_u64(output: &mut String, name: &str, value: Option<u64>) {
    match value {
        Some(value) => write!(output, "\"{name}\":{value},").unwrap(),
        None => write!(output, "\"{name}\":null,").unwrap(),
    }
}
