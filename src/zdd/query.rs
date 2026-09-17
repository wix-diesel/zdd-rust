use super::*;

impl ZddManager {
    pub(crate) fn contains(&self, root: &Root, set: &[u32]) -> bool {
        let zero = self.empty();
        let one = self.unit();
        let mut remainder = root.clone();
        let mut set_index = 0;

        loop {
            if self.roots_equal(&remainder, &zero) {
                return false;
            }
            match self.view(&remainder) {
                RootView::Terminal => {
                    return set_index == set.len() && self.roots_equal(&remainder, &one);
                }
                RootView::Node { variable, hi, lo } => {
                    if set
                        .get(set_index)
                        .is_some_and(|candidate| *candidate < variable)
                    {
                        return false;
                    }
                    if set.get(set_index) == Some(&variable) {
                        remainder = hi;
                        set_index += 1;
                    } else {
                        remainder = lo;
                    }
                }
            }
        }
    }
}
