/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under both the MIT license found in the
 * LICENSE-MIT file in the root directory of this source tree and the Apache
 * License, Version 2.0 found in the LICENSE-APACHE file in the root directory
 * of this source tree.
 */

use elp_ide_db::assists::AssistId;
use elp_ide_db::assists::AssistKind;
use elp_syntax::ast;
use elp_syntax::ast::Spec;
use elp_syntax::AstNode;
use hir::InFile;
use hir::SpecdFunctionDef;

use crate::AssistContext;
use crate::Assists;

// Assist: add_impl
//
// Adds an implementation stub below a spec, if it doesn't already have one.
//
// ```
// -spec foo(Arg1 :: arg1(), arg2()) -> return_type().
// ```
// ->
// ```
// -spec foo(Arg1 :: arg1(), arg2()) -> return_type().
// foo(Arg1, Arg2) ->
//   error("not implemented").
// ```
pub(crate) fn add_impl(acc: &mut Assists, ctx: &AssistContext) -> Option<()> {
    let spec = ctx.find_node_at_offset::<Spec>()?;
    let spec_id = InFile::new(
        ctx.file_id(),
        ctx.sema.find_enclosing_spec(ctx.file_id(), spec.syntax())?,
    );

    let has_impl_already = ctx
        .db()
        .def_map(spec_id.file_id)
        .get_specd_functions()
        .iter()
        .any(|(_, SpecdFunctionDef { spec_def, .. })| spec_def.spec_id == spec_id.value);

    if has_impl_already {
        return None;
    }

    let name = spec.fun()?;
    let name_text = name.text()?;
    let insert = spec.syntax().text_range().end();
    let target = name.syntax().text_range();

    acc.add(
        AssistId("add_impl", AssistKind::Generate),
        "Add implementation",
        target,
        None,
        |builder| {
            let first_sig = spec.sigs().into_iter().next().unwrap();
            let arg_names = first_sig.args().map_or(Vec::new(), |args| {
                args.args()
                    .into_iter()
                    .enumerate()
                    .map(|(arg_idx, expr)| arg_name(arg_idx + 1, expr))
                    .collect()
            });

            match ctx.config.snippet_cap {
                Some(cap) => {
                    let mut snippet_idx = 0;
                    let args_snippets = arg_names
                        .iter()
                        .map(|arg_name| {
                            snippet_idx += 1;
                            format!("${{{}:{}}}, ", snippet_idx, arg_name)
                        })
                        .collect::<String>();
                    snippet_idx += 1;
                    let snippet = format!(
                        "\n{}({}) ->\n  ${{{}:error(\"not implemented\").}}\n",
                        name_text,
                        args_snippets.trim_end_matches(", "),
                        snippet_idx
                    );
                    builder.edit_file(ctx.frange.file_id);
                    builder.insert_snippet(cap, insert, snippet);
                }
                None => {
                    let args_text = arg_names
                        .iter()
                        .map(|arg_name| format!("{}, ", arg_name))
                        .collect::<String>();
                    let text = format!(
                        "\n{}({}) ->\n  error(\"not implemented\").\n",
                        name_text,
                        args_text.trim_end_matches(", ")
                    );
                    builder.edit_file(ctx.frange.file_id);
                    builder.insert(insert, text)
                }
            }
        },
    )
}

pub fn arg_name(arg_idx: usize, expr: ast::Expr) -> String {
    // -spec f(A) -> ok.
    //   f(A) -> ok.
    if let ast::Expr::ExprMax(ast::ExprMax::Var(var)) = expr {
        var.text().to_string()

    // -spec f(A :: foo()) -> ok.
    //   f(A) -> ok.
    } else if let ast::Expr::AnnType(ann) = expr {
        ann.var()
            .and_then(|var| var.var())
            .map(|var| var.text().to_string())
            .unwrap_or_else(|| format!("Arg{}", arg_idx))

    // -spec f(bar()) -> ok.
    //   f(Arg1) -> ok.
    } else {
        format!("Arg{}", arg_idx)
    }
}

#[cfg(test)]
mod tests {
    use expect_test::expect;

    use super::*;
    use crate::tests::*;

    /// We use the "expect parse error" checks below for the cases that generate
    ///   snippets (https://code.visualstudio.com/docs/editor/userdefinedsnippets),
    ///   since the snippets themselves are not valid Erlang code, but Erlang code
    ///   templates consumed by the LSP client to enable quick edits of parameters.

    #[test]
    fn test_base_case() {
        check_assist_expect_parse_error(
            add_impl,
            "Add implementation",
            r#"
-spec ~foo(Foo :: term(), some_atom) -> ok.
"#,
            expect![[r#"
                -spec foo(Foo :: term(), some_atom) -> ok.
                foo(${1:Foo}, ${2:Arg2}) ->
                  ${3:error("not implemented").}

            "#]],
        )
    }

    #[test]
    fn test_previous_has_impl() {
        check_assist_expect_parse_error(
            add_impl,
            "Add implementation",
            r#"
-spec bar() -> ok.
bar() -> ok.
-spec ~foo() -> return_type().
"#,
            expect![[r#"
                -spec bar() -> ok.
                bar() -> ok.
                -spec foo() -> return_type().
                foo() ->
                  ${1:error("not implemented").}

            "#]],
        )
    }

    #[test]
    fn test_already_has_impl_above() {
        check_assist_not_applicable(
            add_impl,
            r#"
foo(Foo, some_atom) -> ok.
-spec ~foo(x(), y()) -> ok.
    "#,
        );
    }

    #[test]
    fn test_already_has_impl_below() {
        check_assist_not_applicable(
            add_impl,
            r#"
-spec ~foo(x(), y()) -> ok.
foo(Foo, some_atom) -> ok.
    "#,
        );
    }
}
