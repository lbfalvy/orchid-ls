use std::ops::Range;
use std::sync::Arc;
use std::{cmp, fmt, iter};

use intern_all::Tok;
use itertools::Itertools;
use orchidlang::location::{SourceCode, SourceRange};

use super::docpos::{bpos2docpos, DocPos};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct SemToken {
  range: SourceRange,
  typ: Tok<String>,
}
impl SemToken {
  pub fn new(range: SourceRange, typ: Tok<String>) -> Self {
    assert!(range.end() <= range.text().len(), "Token is out of bounds");
    Self { range, typ }
  }
  pub fn typ(&self) -> Tok<String> { self.typ.clone() }
  pub fn code(&self) -> SourceCode { self.range.code() }
  pub fn start(&self) -> usize { self.range.start() }
  pub fn end(&self) -> usize { self.range.end() }
  pub fn text(&self) -> Arc<String> { self.range.text() }
  pub fn remap(self, ranges: impl IntoIterator<Item = Range<usize>>) -> impl Iterator<Item = Self> {
    ranges.into_iter().map(move |r| Self::new(self.range.map_range(|_| r), self.typ.clone()))
  }
  pub fn split(self) -> impl IntoIterator<Item = Self> {
    match self.text()[self.start()..self.end()].find('\n') {
      None => vec![self],
      Some(0) => vec![Self::new(self.range.map_range(|r| r.start + 1..r.end), self.typ)],
      Some(sp) => {
        let pre = self.start()..self.start() + sp;
        let post = self.start() + sp + 1..self.end();
        let (h, t) = self.remap([pre, post]).collect_tuple().unwrap();
        iter::once(h).chain(t.split()).collect()
      },
    }
  }

  /// Translate tokens to single-line fragments with absolute line/col positions
  /// and lengths according to VSCode's rules.
  ///
  /// # Panics
  ///
  /// if there are no tokens
  pub fn vscode(tokens: impl IntoIterator<Item = SemToken>) -> Vec<(DocPos, usize, SemToken)> {
    let mut sc = None;
    // Vector of single-line semantic tokens
    let tokens = tokens
      .into_iter()
      .flat_map(|t| t.split())
      .inspect(|t| if let Some(sc) = &sc { assert!(sc == &t.code()) } else { sc = Some(t.code()) })
      .collect_vec();
    let source = sc.expect("transcode_tokens called on 0 tokens").text();
    // Vector of range end numbers paired with a thing that lexically sorts
    // unambiguously
    let halves = (tokens.iter())
        .enumerate()
        .flat_map(|(i, r)| [(r.range.start(), (i, 0)), (r.range.end(), (i, 1))]) // sort key
        .collect_vec();
    // Iter of document ranges paired with the semantic token
    let mut output = (bpos2docpos(halves, &source).into_iter())
        .sorted_unstable_by_key(|t| t.1) // re-sort using the key created above
        .tuples::<(_, _)>()
        .zip_eq(tokens) // panics if the lengths don't match
        .map(|(((start, _), (end, _)), tok)| {
          debug_assert_eq!(end.line, start.line, "Broken above");
          (start, end.char - start.char, tok)
        }).collect_vec();
    output.sort_unstable_by_key(|(start, ..)| *start);
    output
  }
}
impl cmp::Ord for SemToken {
  fn cmp(&self, other: &Self) -> cmp::Ordering {
    self.range.start().cmp(&other.range.start()).then(other.range.end().cmp(&self.range.end()))
  }
}
impl cmp::PartialOrd for SemToken {
  fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> { Some(self.cmp(other)) }
}
impl fmt::Display for SemToken {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}@{:?}", &self.typ, self.range)
  }
}

#[cfg(test)]
mod test {
  use std::ops::Range;
  use std::sync::Arc;

  use intern_all::i;
  use orchidlang::location::{SourceCode, SourceRange};
  use orchidlang::sym;

  use super::SemToken;

  fn s(range: Range<usize>, code: &str) -> Vec<Range<usize>> {
    let sr = SourceRange::new(range, SourceCode::new(sym!(foo), Arc::new(code.to_string())));
    SemToken::new(sr, i!(str: "foo")).split().into_iter().map(|t| t.range.range()).collect()
  }

  #[test]
  #[allow(clippy::single_range_in_vec_init)]
  fn splitting() {
    assert_eq!(s(1..3, "foobarbaz"), [1..3], "No splitting");
    assert_eq!(s(2..7, "foo\nbar\nbaz"), [2..3, 4..7], "1 split ends before newline");
    assert_eq!(s(2..12, "foo\nbar\n\nbaz"), [2..3, 4..7, 9..12], "2 splits through empty line");
  }
}
