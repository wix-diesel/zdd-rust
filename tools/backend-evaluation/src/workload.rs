/// Generate all matchings of a path with `variables` edges.
///
/// Bit `i` represents the edge at variable/level `i`. The generator is shared
/// by both backends, so problem semantics and insertion order cannot diverge.
pub fn path_matchings(variables: u32) -> Vec<u64> {
    assert!(variables <= 63);

    fn visit(
        variable: u32,
        variables: u32,
        previous_selected: bool,
        bits: u64,
        out: &mut Vec<u64>,
    ) {
        if variable == variables {
            out.push(bits);
            return;
        }

        visit(variable + 1, variables, false, bits, out);
        if !previous_selected {
            visit(variable + 1, variables, true, bits | (1 << variable), out);
        }
    }

    let mut out = Vec::new();
    visit(0, variables, false, 0, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_matching_counts_follow_fibonacci() {
        let expected = [1, 2, 3, 5, 8, 13, 21, 34];
        for (variables, expected) in expected.into_iter().enumerate() {
            assert_eq!(path_matchings(variables as u32).len(), expected);
        }
    }
}
