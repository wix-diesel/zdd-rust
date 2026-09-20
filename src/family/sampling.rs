use rand_core::RngCore;

use super::*;

impl SetFamily {
    /// Samples one set uniformly using a caller-provided random number generator.
    ///
    /// This convenience method builds a fresh [`CountIndex`] with default
    /// [`QueryLimits`] on every call. For repeated sampling, build an index
    /// once with [`Self::count_index`] and call [`CountIndex::sample`]. The
    /// index owns an `O(N)` DAG snapshot and exact partial counts, while each
    /// subsequent sample only allocates its rank and result buffers.
    ///
    /// No manager guard is held while `rng` is called, so the generator may
    /// safely re-enter APIs on this family's space. An empty family returns
    /// `Ok(None)` without consuming randomness.
    pub fn sample<R: RngCore>(&self, rng: &mut R) -> Result<Option<Solution>, QueryError> {
        self.count_index(&QueryLimits::default())?.sample(rng)
    }
}

impl CountIndex {
    /// Samples one indexed set uniformly using `rng`.
    ///
    /// The sample is selected with an exact arbitrary-precision rank. Rejection
    /// sampling avoids modulo and floating-point bias, including when the
    /// number of sets exceeds `u128`. Samples are drawn with replacement.
    /// An empty index returns `Ok(None)` without consuming randomness.
    pub fn sample<R: RngCore>(&self, rng: &mut R) -> Result<Option<Solution>, QueryError> {
        let Some(rank) = self.uniform_rank(rng) else {
            return Ok(None);
        };
        Ok(Some(self.solution_at_rank(rank)))
    }

    fn uniform_rank<R: RngCore>(&self, rng: &mut R) -> Option<BigUint> {
        let count = self.count();
        if count.bits() == 0 {
            return None;
        }

        let maximum = count - BigUint::from(1u8);
        let bits = maximum.bits();
        if bits == 0 {
            return Some(BigUint::from(0u8));
        }
        let byte_count = usize::try_from(bits.div_ceil(8))
            .expect("64-bit targets represent BigUint byte lengths");
        let high_bits = (bits % 8) as u8;
        let mut bytes = vec![0u8; byte_count];

        loop {
            rng.fill_bytes(&mut bytes);
            if high_bits != 0 {
                let mask = (1u8 << high_bits) - 1;
                *bytes.last_mut().expect("positive bit length needs a byte") &= mask;
            }
            let rank = BigUint::from_bytes_le(&bytes);
            if &rank < count {
                return Some(rank);
            }
        }
    }

    fn solution_at_rank(&self, mut rank: BigUint) -> Solution {
        let mut reference = self.dag.root;
        let mut elements = Vec::new();
        while reference >= 2 {
            let node = &self.dag.nodes[reference - 2];
            let lo_count = self.counts[node.lo]
                .as_ref()
                .expect("every indexed child has an exact count");
            if &rank < lo_count {
                reference = node.lo;
            } else {
                rank -= lo_count;
                elements.push(VariableId(node.variable));
                reference = node.hi;
            }
        }
        debug_assert_eq!(reference, QUERY_ONE, "a valid rank reaches ONE");
        Solution(elements)
    }
}
