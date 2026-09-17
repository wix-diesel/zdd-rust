use super::*;

impl SetFamily {
    /// Returns the union of two families in the same space.
    pub fn union(&self, other: &Self) -> Result<Self, Error> {
        Ok(self.union_with_stats(other)?.value)
    }

    /// Returns the union and diagnostic statistics.
    pub fn union_with_stats(&self, other: &Self) -> Result<OperationReport<Self>, Error> {
        self.binary_with_stats(other, ApplyOp::Union)
    }

    /// Returns the intersection of two families in the same space.
    pub fn intersection(&self, other: &Self) -> Result<Self, Error> {
        Ok(self.intersection_with_stats(other)?.value)
    }

    /// Returns the intersection and diagnostic statistics.
    pub fn intersection_with_stats(&self, other: &Self) -> Result<OperationReport<Self>, Error> {
        self.binary_with_stats(other, ApplyOp::Intersection)
    }

    /// Returns the sets in this family that are absent from `other`.
    pub fn difference(&self, other: &Self) -> Result<Self, Error> {
        Ok(self.difference_with_stats(other)?.value)
    }

    /// Returns the difference and diagnostic statistics.
    pub fn difference_with_stats(&self, other: &Self) -> Result<OperationReport<Self>, Error> {
        self.binary_with_stats(other, ApplyOp::Difference)
    }

    /// Returns sets that occur in exactly one of the two families.
    pub fn symmetric_difference(&self, other: &Self) -> Result<Self, Error> {
        Ok(self.symmetric_difference_with_stats(other)?.value)
    }

    /// Returns the symmetric difference and diagnostic statistics.
    pub fn symmetric_difference_with_stats(
        &self,
        other: &Self,
    ) -> Result<OperationReport<Self>, Error> {
        self.binary_with_stats(other, ApplyOp::SymmetricDifference)
    }

    /// Returns whether `set` is a member of this family.
    ///
    /// Element order and duplicate elements are normalized.
    pub fn contains(&self, set: &[VariableId]) -> Result<bool, Error> {
        let mut normalized = Vec::with_capacity(set.len());
        for element in set {
            if element.index() >= self.space.variable_count {
                return Err(Error::InvalidElement {
                    index: element.index(),
                    variable_count: self.space.variable_count,
                });
            }
            normalized.push(element.0);
        }
        normalized.sort_unstable();
        normalized.dedup();
        Ok(self.space.manager.contains(&self.root, &normalized))
    }

    /// Keeps only sets containing `element`, without removing it from the sets.
    pub fn filter_contains(&self, element: VariableId) -> Result<Self, Error> {
        self.validate_element(element)?;
        self.filter(FilterSpec::Contains(element.0))
    }

    /// Keeps only sets that do not contain `element`.
    pub fn filter_excludes(&self, element: VariableId) -> Result<Self, Error> {
        self.validate_element(element)?;
        self.filter(FilterSpec::Excludes(element.0))
    }

    /// Keeps only sets that are subsets of `elements`.
    pub fn filter_subsets_of(&self, elements: &[VariableId]) -> Result<Self, Error> {
        let normalized = self.normalize_elements(elements)?;
        self.filter(FilterSpec::Subsets(&normalized))
    }

    /// Keeps only sets that are supersets of `elements`.
    pub fn filter_supersets_of(&self, elements: &[VariableId]) -> Result<Self, Error> {
        let normalized = self.normalize_elements(elements)?;
        if normalized.is_empty() {
            return Ok(self.clone());
        }
        self.filter(FilterSpec::Supersets(&normalized))
    }

    /// Starts a cardinality filter over the number of elements in each set.
    #[must_use]
    pub fn cardinality(&self) -> CardinalityFilter<'_> {
        CardinalityFilter { family: self }
    }
}
