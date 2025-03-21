/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under both the MIT license found in the
 * LICENSE-MIT file in the root directory of this source tree and the Apache
 * License, Version 2.0 found in the LICENSE-APACHE file in the root directory
 * of this source tree.
 */

use std::convert::TryInto;

use elp_syntax::TextRange;
use elp_syntax::TextSize;

use crate::LineIndex;

#[derive(Debug)]
struct Fixme {
    comment_range: TextRange,
    suppression_range: TextRange,
    is_ignore: bool,
}

// serialize as:
// {FixmeCommentStart, FixmeCommentEnd, SuppressionRangeStart, SuppressionRangeEnd, IsIgnore}
impl Into<eetf::Term> for Fixme {
    fn into(self) -> eetf::Term {
        let to_term = |n: TextSize| -> eetf::Term {
            let n: u32 = n.into();
            // eetf::FixInteger holds an i32, which means
            // we can support files with about 2 million LOC
            // otherwise we blow up (calculation based on 1000 chars() per line)
            let n: i32 = n.try_into().unwrap();
            eetf::FixInteger::from(n).into()
        };
        eetf::Tuple::from(vec![
            to_term(self.comment_range.start()),
            to_term(self.comment_range.end()),
            to_term(self.suppression_range.start()),
            to_term(self.suppression_range.end()),
            eetf::Atom {
                name: self.is_ignore.to_string(),
            }
            .into(),
        ])
        .into()
    }
}

pub fn fixmes_eetf(line_index: &LineIndex, file_text: &str) -> eetf::Term {
    let fixmes = collect_fixmes(line_index, file_text);
    let fixmes: Vec<eetf::Term> = fixmes.into_iter().map(|f| f.into()).collect();
    eetf::List::from(fixmes).into()
}

fn collect_fixmes(line_index: &LineIndex, file_text: &str) -> Vec<Fixme> {
    let mut fixmes = Vec::new();
    let pats = vec![("% eqwalizer:fixme", false), ("% eqwalizer:ignore", true)];
    for (pat, is_ignore) in pats {
        let len = pat.len();
        for (i, _) in file_text.match_indices(pat) {
            let start = TextSize::from(i as u32);
            let end = TextSize::from((i + len) as u32);
            let line_num = line_index.line_col(start).line;
            if let Some(suppression_start) = line_index.line_at(line_num as usize + 1) {
                let suppression_end = {
                    let next_next_line_start: u32 = line_index
                        .line_at(line_num as usize + 2)
                        .unwrap_or_else(
                            // end of last line
                            || TextSize::from(file_text.chars().count() as u32),
                        )
                        .into();
                    TextSize::from(next_next_line_start - 1)
                };
                // Temporary for T148094436
                let _pctx = stdx::panic_context::enter(format!("\ncollect_fixmes"));
                let comment_range = TextRange::new(start, end);
                let suppression_range = TextRange::new(suppression_start, suppression_end);
                fixmes.push(Fixme {
                    comment_range,
                    suppression_range,
                    is_ignore,
                });
            }
        }
    }
    fixmes
}
