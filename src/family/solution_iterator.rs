use super::*;

impl SolutionTraversal {
    pub(super) fn new(dag: QueryDag) -> Self {
        let root = dag.root;
        Self {
            dag,
            stack: vec![TraversalStep::Visit(root)],
            current: Vec::new(),
        }
    }

    pub(super) fn next_slice(&mut self) -> Option<&[VariableId]> {
        while let Some(step) = self.stack.pop() {
            match step {
                TraversalStep::Visit(QUERY_ZERO) => {}
                TraversalStep::Visit(QUERY_ONE) => return Some(&self.current),
                TraversalStep::Visit(reference) => {
                    let node = &self.dag.nodes[reference - 2];
                    // LIFO order: enumerate the LO branch completely before HI.
                    self.stack.push(TraversalStep::Include {
                        reference: node.hi,
                        variable: node.variable,
                    });
                    self.stack.push(TraversalStep::Visit(node.lo));
                }
                TraversalStep::Include {
                    reference,
                    variable,
                } => {
                    self.current.push(VariableId(variable));
                    self.stack.push(TraversalStep::Remove);
                    self.stack.push(TraversalStep::Visit(reference));
                }
                TraversalStep::Remove => {
                    self.current
                        .pop()
                        .expect("every traversal removal follows an inclusion");
                }
            }
        }
        None
    }
}

impl Iterator for SolutionIterator {
    type Item = Solution;

    fn next(&mut self) -> Option<Self::Item> {
        self.traversal
            .next_slice()
            .map(|elements| Solution(elements.to_vec()))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, None)
    }
}

impl FusedIterator for SolutionIterator {}
