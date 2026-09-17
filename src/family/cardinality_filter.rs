use super::*;

impl CardinalityFilter<'_> {
    /// Keeps sets containing exactly `count` elements.
    pub fn exactly(&self, count: usize) -> Result<SetFamily, Error> {
        self.between(count..=count)
    }

    /// Keeps sets containing at most `count` elements.
    pub fn at_most(&self, count: usize) -> Result<SetFamily, Error> {
        if count >= self.family.space.variable_count {
            Ok(self.family.clone())
        } else {
            self.family.filter(FilterSpec::Cardinality {
                lower: 0,
                upper: count,
            })
        }
    }

    /// Keeps sets containing at least `count` elements.
    pub fn at_least(&self, count: usize) -> Result<SetFamily, Error> {
        if count == 0 {
            Ok(self.family.clone())
        } else if count > self.family.space.variable_count {
            Ok(SetFamily {
                space: Arc::clone(&self.family.space),
                root: self.family.space.manager.empty(),
            })
        } else {
            self.family.filter(FilterSpec::Cardinality {
                lower: count,
                upper: self.family.space.variable_count,
            })
        }
    }

    /// Keeps sets whose cardinality lies in the inclusive `range`.
    pub fn between(&self, range: std::ops::RangeInclusive<usize>) -> Result<SetFamily, Error> {
        let (start, end) = range.into_inner();
        if start > end {
            return Err(Error::InvalidRange { start, end });
        }
        if start > self.family.space.variable_count {
            return Ok(SetFamily {
                space: Arc::clone(&self.family.space),
                root: self.family.space.manager.empty(),
            });
        }
        let end = end.min(self.family.space.variable_count);
        self.family.filter(FilterSpec::Cardinality {
            lower: start,
            upper: end,
        })
    }
}
