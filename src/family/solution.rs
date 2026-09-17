use super::*;

impl Solution {
    /// Returns the elements in fixed variable order.
    #[must_use]
    pub fn as_slice(&self) -> &[VariableId] {
        &self.0
    }

    /// Consumes the solution and returns its element storage.
    #[must_use]
    pub fn into_vec(self) -> Vec<VariableId> {
        self.0
    }
}

impl AsRef<[VariableId]> for Solution {
    fn as_ref(&self) -> &[VariableId] {
        self.as_slice()
    }
}

impl Deref for Solution {
    type Target = [VariableId];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl IntoIterator for Solution {
    type Item = VariableId;
    type IntoIter = std::vec::IntoIter<VariableId>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Solution {
    type Item = &'a VariableId;
    type IntoIter = std::slice::Iter<'a, VariableId>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
