use super::{
    path::Path,
    path_pattern::{PathPattern, PathPatternMatchReason},
};

#[cfg(test)]
use super::path_pattern::AllowPathPattern;

#[derive(Clone, Debug, PartialEq)]
pub struct Rule<P: PathPattern> {
    patterns: Vec<P>,
}

#[cfg(test)]
impl Rule<AllowPathPattern> {
    pub fn test_value(name: &str) -> Self {
        Self {
            patterns: vec![AllowPathPattern::test_value(name)],
        }
    }
}

impl<P: PathPattern> Rule<P> {
    pub fn create(patterns: Vec<P>) -> Self {
        Self { patterns }
    }
}

impl<P: PathPattern> PathPattern for Rule<P> {
    fn matches(&self, path: &Path) -> (bool, PathPatternMatchReason) {
        self.patterns
            .iter()
            .find_map(|p| {
                if let (true, reason) = p.matches(path) {
                    Some((true, reason))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| (false, String::new()))
    }
}
