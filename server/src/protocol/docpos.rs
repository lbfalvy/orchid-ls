use itertools::Itertools;
use serde::{Deserialize, Serialize};

/// A document position according to LSP. Characters denote utf-16 code points,
/// and lines end with `\r`, `\n` or `\r\n`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct DocPos {
  pub line: usize,
  #[serde(alias = "character")]
  pub char: usize,
}
impl DocPos {
  pub fn new(line: usize, char: usize) -> Self { Self { line, char } }
}

/// Convert LSP document positions into utf-8 byte offsets that can index
/// strings in Rust
///
/// # Panics
///
/// if there are no arguments
#[allow(unused)]
// TODO: semantic highlights will use this, but those need some extensions to
// the macro runner to report which macro consumed a given token
pub fn docpos2bpos<T>(input: impl IntoIterator<Item = (DocPos, T)>, text: &str) -> Vec<(usize, T)> {
  assert!(!text.contains('\r'), "Unicode newlines only");
  let mut sorted = input.into_iter().sorted_unstable_by_key(|p| p.0);
  let mut output = Vec::new();
  let mut cur = sorted.next().unwrap();
  let mut prev_lines_bytes = 0;
  'outer: for (line_i, line) in text.split('\n').enumerate() {
    let mut u16cp = 0;
    let mut line_bytes = 0;
    for c in line.chars() {
      if cur.0.line == line_i {
        assert!(line_i <= cur.0.line, "Points past end of line");
        if line_i == cur.0.line {
          assert!(
            u16cp <= cur.0.char,
            "Points inside a utf-16 codepoint char={:?}, line={}, cp={}",
            cur.0,
            line_i,
            u16cp
          );
          if u16cp == cur.0.char {
            let bpos = prev_lines_bytes + line_bytes;
            output.push((bpos, cur.1));
            'inner: loop {
              // loop to deal with repeat positions
              if let Some(next) = sorted.next() {
                if cur.0 == next.0 {
                  output.push((bpos, next.1));
                  continue;
                }
                cur = next;
                break 'inner;
              }
              break 'outer;
            }
          }
        }
        u16cp += c.len_utf16();
        line_bytes += c.len_utf8();
      }
    }
    prev_lines_bytes += line.len() + 1;
  }
  output
}

/// Convert (utf-8) byte positions into LSP document positions.
///
/// # Panics
///
/// if there are no arguments
pub fn bpos2docpos<T>(input: impl IntoIterator<Item = (usize, T)>, text: &str) -> Vec<(DocPos, T)> {
  assert!(!text.contains('\r'), "Unicode newlines only");
  let mut sorted = input.into_iter().sorted_unstable_by_key(|p| p.0);
  let mut output = Vec::new();
  let mut cur = sorted.next().unwrap();
  let mut bytes = 0;
  'outer: for (line_i, line) in text.split('\n').enumerate() {
    while cur.0 < bytes + line.len() + 1 {
      assert!(bytes <= cur.0, "Skipped over index bytes={bytes}, bpos={}", cur.0);
      let character: usize = line[..(cur.0 - bytes)].chars().map(|c| c.len_utf16()).sum();
      let pos = DocPos::new(line_i, character);
      output.push((pos, cur.1));
      'inner: loop {
        if let Some(c) = sorted.next() {
          assert!(cur.0 <= c.0, "Not sorted!");
          if c.0 == cur.0 {
            output.push((pos, c.1));
            continue;
          }
          cur = c;
          break 'inner;
        }
        break 'outer;
      }
    }
    bytes += line.len() + 1; // for the newline
  }
  output
}

#[cfg(test)]
mod test {
  use super::{bpos2docpos, docpos2bpos, DocPos};

  #[test]
  fn doc2b2doc() {
    let doc_poses = [(DocPos::new(0, 5), 0), (DocPos::new(1, 3), 1), (DocPos::new(1, 7), 2)];
    let text = "Lorem ipsum\ndolor sit amet\nconsectetur adipiscing elit";
    let b_poses = [(5, 0), (15, 1), (19, 2)];
    assert_eq!(docpos2bpos(doc_poses, text), b_poses, "Multiple doc2b");
    assert_eq!(docpos2bpos([(DocPos::new(0, 9), 0)], "Test szöveg"), [(10, 0)], "unicode");
    assert_eq!(bpos2docpos(b_poses, text), doc_poses, "Multiple b2doc");
    assert_eq!(bpos2docpos([(10, 0)], "Test szöveg"), [(DocPos::new(0, 9), 0)], "unicode");
  }
}
