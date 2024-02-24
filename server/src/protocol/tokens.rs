use std::ops::Range;
use std::{cmp, fmt, iter};

use intern_all::Tok;
use itertools::Itertools;
use orchidlang::name::Sym;

use super::docpos::{bpos2docpos, DocPos};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct SemToken {
  pub file: Sym,
  pub range: Range<usize>,
  pub typ: Tok<String>,
  pub mods: Vec<Tok<String>>,
}
impl SemToken {
  pub fn remap(self, ranges: impl IntoIterator<Item = Range<usize>>) -> impl Iterator<Item = Self> {
    ranges.into_iter().map(move |r| Self {
      file: self.file.clone(),
      range: r,
      typ: self.typ.clone(),
      mods: self.mods.clone(),
    })
  }
}
impl cmp::Ord for SemToken {
  fn cmp(&self, other: &Self) -> cmp::Ordering {
    self.range.start.cmp(&other.range.start).then(other.range.end.cmp(&self.range.end))
  }
}
impl cmp::PartialOrd for SemToken {
  fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> { Some(self.cmp(other)) }
}
impl fmt::Display for SemToken {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}@{:?}", iter::once(&self.typ).chain(self.mods.iter()).join("+"), self.range)
  }
}

/// Find the relative position and length of semantic tokens according to VSCode
/// rules.
///
/// # Panics
///
/// if there are no tokens
pub fn transcode_tokens(
  tokens: impl IntoIterator<Item = SemToken>,
  src: &str,
) -> impl Iterator<Item = (DocPos, usize, SemToken)> + '_ {
  // Vector of single-line semantic tokens
  let tokens = tokens
    .into_iter()
    .flat_map(|t| {
      if src.len() < t.range.end {
        panic!("{}@{} is outside {src:?}", t.range.end, t.file);
      }
      t.clone().remap(split_token(t.range.clone(), src))
    })
    .collect_vec();
  // Vector of range end numbers paired with a thing that lexically sorts
  // unambiguously
  let halves = (tokens.iter())
      .enumerate()
      .flat_map(|(i, r)| [(r.range.start, (i, 0)), (r.range.end, (i, 1))]) // sort key
      .collect_vec();
  // Iter of document ranges paired with the semantic token
  (bpos2docpos(halves, src).into_iter())
      .sorted_unstable_by_key(|t| t.1) // re-sort using the key created above
      .tuples::<(_, _)>()
      .zip_eq(tokens) // panics if the lengths don't match
      .map(|(((start, _), (end, _)), tok)| {
        debug_assert_eq!(end.line, start.line, "Broken above");
        (start, end.char - start.char, tok)
      })
      .sorted_unstable_by_key(|(start, _, _)| *start)
}

/// Split a token into distinct tokens that don't contain line breaks
pub fn split_token(token: Range<usize>, source: &str) -> Vec<Range<usize>> {
  match source[token.start..token.end].find('\n') {
    None => vec![token],
    Some(0) => split_token((token.start + 1)..token.end, source),
    Some(sp) => iter::once(token.start..(token.start + sp))
      .chain(split_token((token.start + sp + 1)..token.end, source))
      .collect(),
  }
}

#[cfg(test)]
mod test {
  use super::split_token;

  #[test]
  #[allow(clippy::single_range_in_vec_init)]
  fn splitting() {
    assert_eq!(split_token(1..3, "foobarbaz"), [1..3], "No splitting");
    assert_eq!(split_token(2..7, "foo\nbar\nbaz"), [2..3, 4..7], "1 split ends before newline");
    assert_eq!(
      split_token(2..12, "foo\nbar\n\nbaz"),
      [2..3, 4..7, 9..12],
      "2 splits through empty line"
    );
  }
}
