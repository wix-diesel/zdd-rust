use zdd_family::{BfsOrder, EdgeOrder, EdgeOrdering, Graph, GraphError};

/// A deterministic graph fixture and two deliberately different edge orders.
pub struct GraphDataset {
    pub name: &'static str,
    pub graph: Graph,
    pub good_order: EdgeOrder,
    pub bad_order: EdgeOrder,
}

impl GraphDataset {
    fn new(name: &'static str, graph: Graph) -> Result<Self, GraphError> {
        let good_order = BfsOrder.order(&graph)?;
        let bad_order = EdgeOrder::new(
            &graph,
            (0..graph.edge_count())
                .rev()
                .map(|index| graph.edge_id(index).expect("fixture edge index is valid")),
        )?;
        Ok(Self {
            name,
            graph,
            good_order,
            bad_order,
        })
    }
}

pub fn graph_datasets() -> Result<Vec<GraphDataset>, GraphError> {
    [
        ("chain", chain_edges(24)),
        ("tree", binary_tree_edges(31)),
        ("ladder", ladder_edges(12)),
        ("grid-thin", grid_edges(3, 10)),
        ("grid-square", grid_edges(6, 6)),
        ("complete", complete_edges(8)),
        ("sparse-seed-23", sparse_edges(28, 42, 23)),
    ]
    .into_iter()
    .map(|(name, (vertices, edges))| {
        Graph::from_edges(vertices, edges).and_then(|g| GraphDataset::new(name, g))
    })
    .collect()
}

fn chain_edges(vertices: usize) -> (usize, Vec<(usize, usize)>) {
    (
        vertices,
        (1..vertices).map(|vertex| (vertex - 1, vertex)).collect(),
    )
}

fn binary_tree_edges(vertices: usize) -> (usize, Vec<(usize, usize)>) {
    (
        vertices,
        (1..vertices)
            .map(|child| ((child - 1) / 2, child))
            .collect(),
    )
}

fn ladder_edges(rungs: usize) -> (usize, Vec<(usize, usize)>) {
    let mut edges = Vec::with_capacity(rungs * 3 - 2);
    for rung in 0..rungs {
        edges.push((rung, rung + rungs));
        if rung + 1 < rungs {
            edges.push((rung, rung + 1));
            edges.push((rung + rungs, rung + rungs + 1));
        }
    }
    (rungs * 2, edges)
}

fn grid_edges(rows: usize, columns: usize) -> (usize, Vec<(usize, usize)>) {
    let mut edges = Vec::new();
    for row in 0..rows {
        for column in 0..columns {
            let vertex = row * columns + column;
            if row + 1 < rows {
                edges.push((vertex, vertex + columns));
            }
            if column + 1 < columns {
                edges.push((vertex, vertex + 1));
            }
        }
    }
    (rows * columns, edges)
}

fn complete_edges(vertices: usize) -> (usize, Vec<(usize, usize)>) {
    (
        vertices,
        (0..vertices)
            .flat_map(|first| ((first + 1)..vertices).map(move |second| (first, second)))
            .collect(),
    )
}

fn sparse_edges(vertices: usize, edge_count: usize, seed: u64) -> (usize, Vec<(usize, usize)>) {
    let mut state = seed;
    let mut edges = Vec::with_capacity(edge_count);
    while edges.len() < edge_count {
        state = xorshift(state);
        let first = state as usize % vertices;
        state = xorshift(state);
        let second = state as usize % vertices;
        let edge = if first < second {
            (first, second)
        } else {
            (second, first)
        };
        if edge.0 != edge.1 && !edges.contains(&edge) {
            edges.push(edge);
        }
    }
    (vertices, edges)
}

fn xorshift(mut value: u64) -> u64 {
    value ^= value << 13;
    value ^= value >> 7;
    value ^ (value << 17)
}
