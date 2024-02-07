use itertools::Itertools;
use serde::{Deserialize, Serialize};

/// Entries in `workspaceEntries` on init
#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
pub struct WspaceEnt {
  pub name: String,
  pub uri: String,
}

/// A document position according to LSP. Characters denote utf-16 code points,
/// and lines end with `\r`, `\n` or `\r\n`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct DocPos {
  pub line: usize,
  pub character: usize,
}
impl DocPos {
  pub fn to_byte_i<const N: usize>(positions: [Self; N], text: &str) -> [usize; N] {
    assert!(!text.contains('\r'), "Unicode newlines only");
    let mut pos_idx_tbl = positions.into_iter().enumerate().collect_vec();
    pos_idx_tbl.sort_unstable_by(|p1, p2| p2.1.cmp(&p1.1));
    let mut cur = pos_idx_tbl.pop().unwrap();
    let mut bpos_idx_tbl = Vec::<(usize, usize)>::with_capacity(N);
    let mut bytes = 0;
    'outer: for (line_i, line) in text.split('\n').enumerate() {
      if cur.1.line < line_i {
        bytes += line.len();
        continue;
      }
      let mut u16cp = 0;
      let mut l_bytes = 0;
      for c in line.chars() {
        assert!(cur.1.line <= line_i, "Points past end of line");
        if cur.1.line == line_i {
          assert!(cur.1.character <= u16cp, "Points inside a utf-16 codepoint");
          if cur.1.character == u16cp {
            bpos_idx_tbl.push((cur.0, bytes + l_bytes));
            'inner: loop {
              // loop to deal with repeat positions in iterating pattern
              if let Some(c) = pos_idx_tbl.pop() {
                if cur == c {
                  bpos_idx_tbl.push((c.0, bytes + l_bytes));
                  continue;
                }
                cur = c;
                break 'inner;
              }
              break 'outer;
            }
          }
        }
        u16cp += c.len_utf16();
        l_bytes += c.len_utf8();
      }
      bytes += l_bytes + 1; // for the newline
    }
    bpos_idx_tbl.sort_unstable_by_key(|p| p.0);
    bpos_idx_tbl.into_iter().map(|p| p.1).collect_vec().try_into().expect("Same number in as out")
  }

  pub fn from_byte_i<const N: usize>(positions: [usize; N], text: &str) -> [Self; N] {
    assert!(!text.contains('\r'), "Unicode newlines only");
    positions.map(|pos| {
      let (prev, _) = text.split_at(pos);
      Self {
        line: prev.chars().filter(|c| *c == '\n').count(),
        character: prev.chars().rev().take_while(|c| *c != '\n').count(),
      }
    })
  }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct DocRange {
  pub start: DocPos,
  pub end: DocPos,
}

#[derive(Serialize, Deserialize)]
pub struct TextDocumentItem {
  pub uri: String,
  #[serde(alias = "languageId")]
  pub language_id: String,
  pub version: i64,
  pub text: String,
}


