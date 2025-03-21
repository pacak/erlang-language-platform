/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under both the MIT license found in the
 * LICENSE-MIT file in the root directory of this source tree and the Apache
 * License, Version 2.0 found in the LICENSE-APACHE file in the root directory
 * of this source tree.
 */

use elp_ide_db::elp_base_db::FilePosition;
use elp_ide_db::RootDatabase;
use elp_ide_db::SymbolClass;
use elp_ide_db::SymbolDefinition;
use elp_syntax::AstNode;
use hir::InFile;
use hir::Semantic;

const OTP_BASE_URL: &str = "https://erlang.org";

/// Retrieve a link to documentation for the given symbol.
pub(crate) fn external_docs(db: &RootDatabase, position: &FilePosition) -> Option<Vec<String>> {
    let sema = Semantic::new(db);
    let source_file = sema.parse(position.file_id);

    // Temporary for T153426323
    let _pctx = stdx::panic_context::enter(format!("\nexternal_docs"));
    let token = source_file
        .value
        .syntax()
        .token_at_offset(position.offset)
        .left_biased()?;

    let doc_links = SymbolClass::classify(&sema, InFile::new(position.file_id, token))?
        .into_iter()
        .filter_map(|def| doc_links(&sema, def))
        .flatten()
        .collect();
    Some(doc_links)
}

fn doc_links(sema: &Semantic, def: SymbolDefinition) -> Option<Vec<String>> {
    match def {
        SymbolDefinition::Module(module) => {
            if module.is_in_otp(sema.db) {
                let url = format!("{}/doc/man/{}.html", OTP_BASE_URL, module.name(sema.db));
                Some(vec![url])
            } else {
                None
            }
        }
        SymbolDefinition::Function(function_def) => {
            if function_def.is_in_otp(sema.db) {
                let module_name = sema.module_name(function_def.file.file_id)?;
                let url = format!(
                    "{}/doc/man/{}.html#{}-{}",
                    OTP_BASE_URL,
                    module_name.as_str(),
                    function_def.function.name.name(),
                    function_def.function.name.arity()
                );
                Some(vec![url])
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::fixture;

    fn check(fixture: &str, expected_links: Vec<&str>) {
        let (analysis, position) = fixture::position(fixture);
        let actual_links = analysis.external_docs(position).ok().unwrap().unwrap();
        assert_eq!(actual_links, expected_links);
    }

    #[test]
    fn otp_module_doc_links() {
        check(
            r#"
//- /opt/lib/stdlib-3.17/src/lists.erl otp_app:/opt/lib/stdlib-3.17
-module(lists).
-export([reverse/1]).
reverse([]) -> [].

//- /src/two.erl
-module(two).
a() ->
  list~s:reverse([]).
        "#,
            vec!["https://erlang.org/doc/man/lists.html"],
        )
    }

    #[test]
    fn non_otp_module_doc_links() {
        check(
            r#"
//- /src/one.erl
-module(one).
-export([reverse/1]).
reverse([]) -> [].

//- /src/two.erl
-module(two).
a() ->
  on~e:reverse([]).
        "#,
            vec![],
        )
    }

    #[test]
    fn otp_function_doc_links() {
        check(
            r#"
//- /opt/lib/stdlib-3.17/src/lists.erl otp_app:/opt/lib/stdlib-3.17
-module(lists).
-export([reverse/1]).
reverse([]) -> [].

//- /src/two.erl
-module(two).
a() ->
  lists:rev~erse([]).
        "#,
            vec!["https://erlang.org/doc/man/lists.html#reverse-1"],
        )
    }

    #[test]
    fn non_otp_function_doc_links() {
        check(
            r#"
//- /src/one.erl
-module(one).
-export([reverse/1]).
reverse([]) -> [].

//- /src/two.erl
-module(two).
a() ->
  one:rev~erse([]).
        "#,
            vec![],
        )
    }
}
