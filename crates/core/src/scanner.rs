#[derive(Debug, PartialEq)]
pub enum EolKind {
    Cr,
    Lf,
    CrLf,
}

impl EolKind {
    pub const fn len(&self) -> usize {
        match self {
            EolKind::Cr => 1,
            EolKind::Lf => 1,
            EolKind::CrLf => 2,
        }
    }
}

/// Represents the description for all non-final lines.
#[derive(Debug, PartialEq)]
pub struct OtherLineSpan {
    offset: usize,
    length: usize,
    eol: EolKind,
}

impl OtherLineSpan {
    fn new(offset: usize, length: usize, eol: EolKind) -> Self {
        Self {
            offset,
            length,
            eol,
        }
    }
}

/// Represents the description for the last line within source text.
#[derive(Debug, PartialEq)]
pub struct LastLineSpan {
    offset: usize,
    length: usize,
}

impl LastLineSpan {
    fn new(offset: usize, length: usize) -> Self {
        Self { offset, length }
    }
}

/// Represents a location within the source text, which contains both row and column numbers.
#[derive(Debug, PartialEq)]
pub struct Location {
    pub row: usize,
    pub column: usize,
}

impl Location {
    pub fn new(row: usize, column: usize) -> Location {
        Location { row, column }
    }
}

/// Stores offset, length, and EOL kind for each line within the source text, can be used for
/// converting offset into row and column numbers.
#[derive(Debug, PartialEq)]
pub struct SourceLayout {
    /// Total length of the source text in characters.
    source_len: usize,

    /// Description for all non-last lines.
    other_lines: Vec<OtherLineSpan>,

    /// Description for last line.
    last_line: LastLineSpan,
}

impl SourceLayout {
    /// Gets the length in characters.
    pub fn len(&self) -> usize {
        self.source_len
    }

    /// Gets the number of lines.
    pub fn count(&self) -> usize {
        self.other_lines.len() + 1
    }

    /// Gets the row and column numbers for the given offset.
    pub fn locate(&self, offset: usize) -> Option<Location> {
        if offset > self.source_len {
            None
        } else if offset == self.source_len {
            Some(Location::new(self.other_lines.len(), self.last_line.length))
        } else if self.other_lines.len() == 0 {
            Some(Location::new(0, offset))
        } else {
            let index_max = self.other_lines.len() - 1;
            let mut index_start = 0;
            let mut index_end = index_max;
            loop {
                let index_middle = index_start + (index_end - index_start) / 2;
                let OtherLineSpan {
                    offset: line_offset,
                    length: line_length,
                    eol,
                } = &self.other_lines[index_middle];

                if offset < *line_offset {
                    index_end = index_middle - 1;
                } else if offset >= *line_offset + *line_length + eol.len() {
                    if index_middle == index_max {
                        return Some(Location::new(
                            index_middle + 1,
                            offset - self.last_line.offset,
                        ));
                    } else {
                        index_start = index_middle + 1;
                    }
                } else {
                    let a = offset - *line_offset;
                    return if a <= *line_length {
                        Some(Location::new(index_middle, a))
                    } else {
                        Some(Location::new(index_middle + 1, 0))
                    };
                }
            }
        }
    }
}

impl Default for SourceLayout {
    fn default() -> Self {
        Self {
            source_len: 0,
            other_lines: vec![],
            last_line: LastLineSpan::new(0, 0),
        }
    }
}

enum State {
    Start,
    Cr,
}

enum Action {
    Continue,
    Again,
}

struct Scanner<'s> {
    source: &'s str,
    state: State,
    offset: usize,
    start: usize,
    info: SourceLayout,
}

impl<'s> Scanner<'s> {
    fn new(source: &'s str) -> Self {
        Self {
            source,
            state: State::Start,
            offset: 0,
            start: 0,
            info: Default::default(),
        }
    }

    fn add_line(&mut self, eol: EolKind) {
        let old_start = self.start;

        let mut length = self.offset - self.start;
        match eol {
            EolKind::Cr => {
                length -= 1;
                self.start = self.offset;
            }
            EolKind::Lf => {
                self.start = self.offset + 1;
            }
            EolKind::CrLf => {
                length -= 1;
                self.start = self.offset + 1;
            }
        }

        self.info
            .other_lines
            .push(OtherLineSpan::new(old_start, length, eol));
    }

    fn feed_char(&mut self, c: char) -> Action {
        match self.state {
            State::Start => match c {
                '\r' => {
                    self.state = State::Cr;
                    Action::Continue
                }
                '\n' => {
                    self.add_line(EolKind::Lf);
                    Action::Continue
                }
                _ => Action::Continue,
            },
            State::Cr => match c {
                '\n' => {
                    self.add_line(EolKind::CrLf);
                    self.state = State::Start;
                    Action::Continue
                }
                _ => {
                    self.add_line(EolKind::Cr);
                    self.state = State::Start;
                    Action::Again
                }
            },
        }
    }

    fn feed_eos(&mut self) {
        match self.state {
            State::Start => {
                self.info.last_line.offset = self.start;
                self.info.last_line.length = self.offset - self.start;
            }
            State::Cr => {
                let old_start = self.start;
                let length = self.offset - self.start - 1;
                self.start = self.offset;

                self.info
                    .other_lines
                    .push(OtherLineSpan::new(old_start, length, EolKind::Cr));

                self.info.last_line.offset = self.start;
            }
        }
    }

    fn scan(&mut self) -> SourceLayout {
        for c in self.source.chars() {
            loop {
                match self.feed_char(c) {
                    Action::Continue => break,
                    Action::Again => continue,
                }
            }

            self.offset += 1;
        }

        self.feed_eos();

        self.info.source_len = self.offset;
        std::mem::take(&mut self.info)
    }
}

/// Scans source text and outputs the source layout.
pub fn scan(source: &str) -> SourceLayout {
    Scanner::new(source).scan()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// OLS (Other-Line Span), macro for creating `OtherLineSpan`.
    macro_rules! ols {
        ($off:expr, $len:expr, $eol:expr) => {
            OtherLineSpan::new($off, $len, $eol)
        };
    }

    /// Macro for creating `OtherLineSpan`, in which the EOL is CR.
    macro_rules! ols_cr {
        ($off:expr, $len:expr) => {
            ols!($off, $len, EolKind::Cr)
        };
    }

    /// Macro for creating `OtherLineSpan`, in which the EOL is LF.
    macro_rules! ols_lf {
        ($off:expr, $len:expr) => {
            ols!($off, $len, EolKind::Lf)
        };
    }

    /// Macro for creating `OtherLineSpan`, in which the EOL is CRLF.
    macro_rules! ols_crlf {
        ($off:expr, $len:expr) => {
            ols!($off, $len, EolKind::CrLf)
        };
    }

    /// LLS (Last-Line Span), macro for creating `LastLineSpan`.
    macro_rules! lls {
        ($off:expr, $len:expr) => {
            LastLineSpan::new($off, $len)
        };
    }

    /// SL (Source Layout), macro for creating `SourceLayout`.
    macro_rules! sl {
        ($sl:expr, [$($oi:expr),* $(,)?], $li:expr) => {
            SourceLayout {
                source_len: $sl,
                other_lines: vec![$($oi),*],
                last_line: $li,
            }
        };
    }

    macro_rules! loc {
        ($row:expr, $column:expr) => {
            Location::new($row, $column)
        };
    }

    #[test]
    fn scanning_ascii() {
        let mut text;
        let mut info;

        text = "";
        info = scan(text);
        assert_eq!(info, sl!(0, [], lls!(0, 0)));
        assert_eq!(info.count(), 1);
        assert_eq!(info.locate(0), Some(loc!(0, 0)));
        assert_eq!(info.locate(1), None);

        text = "Hello, world!";
        info = scan(text);
        assert_eq!(info, sl!(13, [], lls!(0, 13)));
        assert_eq!(info.count(), 1);
        assert_eq!(info.locate(0), Some(loc!(0, 0)));
        assert_eq!(info.locate(5), Some(loc!(0, 5)));
        assert_eq!(info.locate(13), Some(loc!(0, 13)));
        assert_eq!(info.locate(14), None);

        text = "Hello, world!\r";
        info = scan(text);
        assert_eq!(info, sl!(14, [ols_cr!(0, 13)], lls!(14, 0)));
        assert_eq!(info.count(), 2);
        assert_eq!(info.locate(0), Some(loc!(0, 0)));
        assert_eq!(info.locate(5), Some(loc!(0, 5)));
        assert_eq!(info.locate(13), Some(loc!(0, 13)));
        assert_eq!(info.locate(14), Some(loc!(1, 0)));
        assert_eq!(info.locate(15), None);

        text = "Hello, world!\n";
        info = scan(text);
        assert_eq!(info, sl!(14, [ols_lf!(0, 13)], lls!(14, 0)));
        assert_eq!(info.count(), 2);

        text = "Hello, world!\r\n";
        info = scan(text);
        assert_eq!(info, sl!(15, [ols_crlf!(0, 13)], lls!(15, 0)));
        assert_eq!(info.count(), 2);
        assert_eq!(info.locate(0), Some(loc!(0, 0)));
        assert_eq!(info.locate(5), Some(loc!(0, 5)));
        assert_eq!(info.locate(13), Some(loc!(0, 13)));
        assert_eq!(info.locate(14), Some(loc!(1, 0)));
        assert_eq!(info.locate(15), Some(loc!(1, 0)));
        assert_eq!(info.locate(16), None);

        text = "Hello, world!\rMy name is Alex!";
        info = scan(text);
        assert_eq!(info, sl!(30, [ols_cr!(0, 13)], lls!(14, 16)));
        assert_eq!(info.count(), 2);
        assert_eq!(info.locate(0), Some(loc!(0, 0)));
        assert_eq!(info.locate(5), Some(loc!(0, 5)));
        assert_eq!(info.locate(13), Some(loc!(0, 13)));
        assert_eq!(info.locate(14), Some(loc!(1, 0)));
        assert_eq!(info.locate(17), Some(loc!(1, 3)));
        assert_eq!(info.locate(30), Some(loc!(1, 16)));
        assert_eq!(info.locate(31), None);

        text = "Hello, world!\nMy name is Alex!";
        info = scan(text);
        assert_eq!(info, sl!(30, [ols_lf!(0, 13)], lls!(14, 16)));
        assert_eq!(info.count(), 2);

        text = "Hello, world!\r\nMy name is Alex!";
        info = scan(text);
        assert_eq!(info, sl!(31, [ols_crlf!(0, 13)], lls!(15, 16)));
        assert_eq!(info.count(), 2);

        text = "Hello, world!\rMy name is Alex!\r";
        info = scan(text);
        assert_eq!(
            info,
            sl!(31, [ols_cr!(0, 13), ols_cr!(14, 16)], lls!(31, 0))
        );
        assert_eq!(info.count(), 3);
        assert_eq!(info.locate(0), Some(loc!(0, 0)));
        assert_eq!(info.locate(5), Some(loc!(0, 5)));
        assert_eq!(info.locate(13), Some(loc!(0, 13)));
        assert_eq!(info.locate(14), Some(loc!(1, 0)));
        assert_eq!(info.locate(17), Some(loc!(1, 3)));
        assert_eq!(info.locate(30), Some(loc!(1, 16)));
        assert_eq!(info.locate(31), Some(loc!(2, 0)));
        assert_eq!(info.locate(32), None);

        text = "Hello, world!\rMy name is Alex!\n";
        info = scan(text);
        assert_eq!(
            info,
            sl!(31, [ols_cr!(0, 13), ols_lf!(14, 16)], lls!(31, 0))
        );
        assert_eq!(info.count(), 3);

        text = "Hello, world!\rMy name is Alex!\r\n";
        info = scan(text);
        assert_eq!(
            info,
            sl!(32, [ols_cr!(0, 13), ols_crlf!(14, 16)], lls!(32, 0))
        );
        assert_eq!(info.count(), 3);
    }

    #[test]
    fn scanning_unicode() {
        let text;
        let info;

        text = "¡Hola, 世界!\r我的名字是Alex。\r\n";
        info = scan(text);
        assert_eq!(
            info,
            sl!(23, [ols_cr!(0, 10), ols_crlf!(11, 10)], lls!(23, 0))
        );
        assert_eq!(info.count(), 3);
    }
}
