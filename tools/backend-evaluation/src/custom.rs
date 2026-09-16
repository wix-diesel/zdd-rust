use std::collections::HashMap;
use std::time::{Duration, Instant};

const ZERO: usize = 0;
const ONE: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Node {
    variable: u32,
    low: usize,
    high: usize,
}

struct Core {
    nodes: Vec<Option<Node>>,
    unique: HashMap<Node, usize>,
    node_limit: usize,
}

impl Core {
    fn new(node_limit: usize) -> Self {
        Self {
            nodes: vec![None, None],
            unique: HashMap::new(),
            node_limit,
        }
    }

    fn node(&self, id: usize) -> Option<Node> {
        self.nodes[id]
    }

    fn make_node(&mut self, variable: u32, low: usize, high: usize) -> Result<usize, &'static str> {
        if high == ZERO {
            return Ok(low);
        }
        let node = Node {
            variable,
            low,
            high,
        };
        if let Some(&id) = self.unique.get(&node) {
            return Ok(id);
        }
        if self.nodes.len() - 2 >= self.node_limit {
            return Err("node limit");
        }
        let id = self.nodes.len();
        self.nodes.push(Some(node));
        self.unique.insert(node, id);
        Ok(id)
    }

    fn singleton_set(&mut self, bits: u64, variables: u32) -> Result<usize, &'static str> {
        let mut root = ONE;
        for variable in (0..variables).rev() {
            if bits & (1 << variable) != 0 {
                root = self.make_node(variable, ZERO, root)?;
            }
        }
        Ok(root)
    }

    fn powerset(&mut self, variables: u32) -> Result<usize, &'static str> {
        let mut root = ONE;
        for variable in (0..variables).rev() {
            root = self.make_node(variable, root, root)?;
        }
        Ok(root)
    }

    fn union(&mut self, left: usize, right: usize) -> Result<usize, &'static str> {
        fn apply(
            core: &mut Core,
            left: usize,
            right: usize,
            memo: &mut HashMap<(usize, usize), usize>,
        ) -> Result<usize, &'static str> {
            if left == ZERO {
                return Ok(right);
            }
            if right == ZERO || left == right {
                return Ok(left);
            }
            let key = if left < right {
                (left, right)
            } else {
                (right, left)
            };
            if let Some(&result) = memo.get(&key) {
                return Ok(result);
            }

            let left_node = core.node(left);
            let right_node = core.node(right);
            let level = match (left_node, right_node) {
                (Some(left), Some(right)) => left.variable.min(right.variable),
                (Some(left), None) => left.variable,
                (None, Some(right)) => right.variable,
                (None, None) => return Ok(ONE),
            };
            let (left_low, left_high) = match left_node {
                Some(node) if node.variable == level => (node.low, node.high),
                _ => (left, ZERO),
            };
            let (right_low, right_high) = match right_node {
                Some(node) if node.variable == level => (node.low, node.high),
                _ => (right, ZERO),
            };
            let low = apply(core, left_low, right_low, memo)?;
            let high = apply(core, left_high, right_high, memo)?;
            let result = core.make_node(level, low, high)?;
            memo.insert(key, result);
            Ok(result)
        }

        apply(self, left, right, &mut HashMap::new())
    }

    fn filter_contains(&mut self, root: usize, target: u32) -> Result<usize, &'static str> {
        fn apply(
            core: &mut Core,
            root: usize,
            target: u32,
            memo: &mut HashMap<usize, usize>,
        ) -> Result<usize, &'static str> {
            if root <= ONE {
                return Ok(ZERO);
            }
            if let Some(&result) = memo.get(&root) {
                return Ok(result);
            }
            let node = core.node(root).unwrap();
            let result = if node.variable == target {
                core.make_node(node.variable, ZERO, node.high)?
            } else if node.variable > target {
                ZERO
            } else {
                let low = apply(core, node.low, target, memo)?;
                let high = apply(core, node.high, target, memo)?;
                core.make_node(node.variable, low, high)?
            };
            memo.insert(root, result);
            Ok(result)
        }

        apply(self, root, target, &mut HashMap::new())
    }

    fn filter_excludes(&mut self, root: usize, target: u32) -> Result<usize, &'static str> {
        fn apply(
            core: &mut Core,
            root: usize,
            target: u32,
            memo: &mut HashMap<usize, usize>,
        ) -> Result<usize, &'static str> {
            if root <= ONE {
                return Ok(root);
            }
            if let Some(&result) = memo.get(&root) {
                return Ok(result);
            }
            let node = core.node(root).unwrap();
            let result = if node.variable == target {
                node.low
            } else if node.variable > target {
                root
            } else {
                let low = apply(core, node.low, target, memo)?;
                let high = apply(core, node.high, target, memo)?;
                core.make_node(node.variable, low, high)?
            };
            memo.insert(root, result);
            Ok(result)
        }

        apply(self, root, target, &mut HashMap::new())
    }

    fn count(&self, root: usize) -> u128 {
        fn visit(core: &Core, root: usize, memo: &mut HashMap<usize, u128>) -> u128 {
            if root == ZERO {
                return 0;
            }
            if root == ONE {
                return 1;
            }
            if let Some(&count) = memo.get(&root) {
                return count;
            }
            let node = core.node(root).unwrap();
            let count = visit(core, node.low, memo) + visit(core, node.high, memo);
            memo.insert(root, count);
            count
        }

        visit(self, root, &mut HashMap::new())
    }
}

pub struct ResultRow {
    pub build: Duration,
    pub filters: Duration,
    pub checksum: u128,
    pub nodes_after_build: usize,
    pub nodes_after_filters: usize,
}

pub fn run(variables: u32, rounds: u32, sets: &[u64]) -> Result<ResultRow, &'static str> {
    let mut semantics = Core::new(256);
    assert_eq!(semantics.count(ZERO), 0);
    assert_eq!(semantics.count(ONE), 1);
    let powerset = semantics.powerset(4)?;
    assert_eq!(semantics.count(powerset), 16);
    let skipped = semantics.singleton_set(0b1000, 4)?;
    assert_eq!(semantics.count(skipped), 1);

    let mut core = Core::new(1_000_000);
    let start = Instant::now();
    let mut root = ZERO;
    for &bits in sets {
        let set = core.singleton_set(bits, variables)?;
        root = core.union(root, set)?;
    }
    let build = start.elapsed();
    let nodes_after_build = core.nodes.len() - 2;
    assert_eq!(core.count(root), sets.len() as u128);

    let start = Instant::now();
    let mut checksum = 0;
    for _ in 0..rounds {
        for variable in 0..variables {
            let included = core.filter_contains(root, variable)?;
            let excluded = core.filter_excludes(root, variable)?;
            checksum += core.count(included);
            checksum += core.count(excluded);
        }
    }
    let filters = start.elapsed();

    // A failed operation may leave valid nodes, but must not alter old roots.
    // Keep this check independent of the workload's CLI-controlled universe.
    const LIMIT_CHECK_VARIABLES: u32 = 8;
    let mut limited = Core::new(4);
    let stable = limited.singleton_set(1, LIMIT_CHECK_VARIABLES)?;
    let before = limited.count(stable);
    let failure = limited.singleton_set(u64::MAX, LIMIT_CHECK_VARIABLES);
    assert!(failure.is_err());
    assert_eq!(limited.count(stable), before);

    Ok(ResultRow {
        build,
        filters,
        checksum,
        nodes_after_build,
        nodes_after_filters: core.nodes.len() - 2,
    })
}
