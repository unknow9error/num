#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Span {
    pub source: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

impl Span {
    pub fn new(
        source: impl Into<String>,
        start: usize,
        end: usize,
        line: usize,
        column: usize,
    ) -> Self {
        Self {
            source: source.into(),
            start,
            end,
            line,
            column,
        }
    }

    pub fn synthetic(source: impl Into<String>) -> Self {
        Self::new(source, 0, 0, 1, 1)
    }
}
