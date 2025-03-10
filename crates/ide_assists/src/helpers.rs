/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under both the MIT license found in the
 * LICENSE-MIT file in the root directory of this source tree and the Apache
 * License, Version 2.0 found in the LICENSE-APACHE file in the root directory
 * of this source tree.
 */

use std::iter;

use elp_ide_db::elp_base_db::FileId;
use elp_ide_db::rename::is_safe_function;
use elp_ide_db::source_change::SourceChangeBuilder;
use elp_ide_db::ReferenceClass;
use elp_ide_db::SymbolClass;
use elp_ide_db::SymbolDefinition;
use elp_syntax::ast;
use elp_syntax::match_ast;
use elp_syntax::AstNode;
use elp_syntax::AstPtr;
use elp_syntax::NodeOrToken;
use elp_syntax::SourceFile;
use elp_syntax::SyntaxElement;
use elp_syntax::SyntaxKind;
use elp_syntax::SyntaxNode;
use elp_syntax::TextRange;
use fxhash::FxHashSet;
use hir::Clause;
use hir::CompileOption;
use hir::FormList;
use hir::InFileAstPtr;
use hir::InFunctionBody;
use hir::NameArity;
use hir::Semantic;
use hir::Var;
use text_edit::TextSize;

use crate::assist_context::AssistContext;

pub fn prev_form_nodes(syntax: &SyntaxNode) -> impl Iterator<Item = SyntaxNode> {
    syntax
        .siblings_with_tokens(elp_syntax::Direction::Prev)
        .skip(1) // Starts with itself
        .filter_map(|node_or_token| node_or_token.into_node())
        .take_while(|node| node.kind() != SyntaxKind::FUN_DECL)
}

/// Use surrounding context to suggest a name for a new variable.
/// Defaults to simply `VarName` for now.
///
/// **NOTE**: it is caller's responsibility to guarantee uniqueness of the name.
/// I.e. it doesn't look for names in scope.
pub(crate) fn suggest_name_for_variable(_expr: &ast::Expr, _sema: &Semantic) -> String {
    "VarName".to_string()
}

/// Given a variable name and vars in scope, return either the
/// original if it does not clash, or one with the smallest numeric suffix to be fresh.
pub(crate) fn freshen_variable_name(
    sema: &Semantic,
    var_name: String,
    vars_in_clause: &Option<FxHashSet<Var>>,
) -> String {
    if let Some(vars_in_clause) = vars_in_clause {
        let is_safe = |name: &String| -> bool {
            vars_in_clause
                .iter()
                .all(|v| name != &v.as_string(sema.db.upcast()))
        };
        if is_safe(&var_name) {
            var_name
        } else {
            let mut i = 0;
            loop {
                let name = format!("{var_name}{i}").to_string();
                if is_safe(&name) {
                    return name;
                }
                i = i + 1;
            }
        }
    } else {
        var_name
    }
}

/// Given a function name/arity and FileId, return either the original if it
/// does not clash, or one with the smallest numeric suffix to be
/// fresh.
pub(crate) fn freshen_function_name(ctx: &AssistContext, name: String, arity: u32) -> String {
    if is_safe_function(&ctx.sema, ctx.file_id(), &name, arity) {
        name
    } else {
        let mut i = 0;
        loop {
            let candidate_name = format!("{name}_{i}").to_string();
            if is_safe_function(&ctx.sema, ctx.file_id(), &candidate_name, arity) {
                return candidate_name;
            }
            i = i + 1;
        }
    }
}

pub(crate) fn skip_ws(node: Option<NodeOrToken>) -> Option<TextRange> {
    node.and_then(SyntaxElement::into_token).and_then(|t| {
        if t.kind() == SyntaxKind::WHITESPACE {
            Some(t.text_range())
        } else {
            None
        }
    })
}

pub(crate) fn skip_trailing_separator(node: &SyntaxNode) -> Option<TextRange> {
    let elements = iter::successors(node.next_sibling_or_token(), |n| {
        (*n).next_sibling_or_token()
    });
    for element in elements {
        if let Some(t) = &SyntaxElement::into_token(element) {
            if t.kind() != SyntaxKind::WHITESPACE {
                return Some(t.text_range());
            }
        }
    }
    None
}

pub(crate) fn find_next_token(node: &SyntaxNode, delimiter: SyntaxKind) -> Option<TextRange> {
    node.children_with_tokens()
        .filter_map(|it| it.into_token())
        .filter(|it| it.kind() == delimiter)
        .next()
        .map(|t| t.text_range())
}

pub(crate) fn skip_trailing_newline(node: &SyntaxNode) -> Option<TextRange> {
    let elements = iter::successors(node.next_sibling_or_token(), |n| {
        (*n).next_sibling_or_token()
    });
    for element in elements {
        if let Some(t) = &SyntaxElement::into_token(element) {
            if t.kind() == SyntaxKind::WHITESPACE && t.text().contains("\n") {
                return Some(t.text_range());
            }
        } else {
            return None;
        }
    }
    None
}

pub(crate) fn parens_needed(expr: &ast::Expr, var: &ast::Var) -> Option<(TextRange, bool)> {
    let rhs_not_needed = matches!(
        expr,
        ast::Expr::ExprMax(ast::ExprMax::Atom(_))
            | ast::Expr::ExprMax(ast::ExprMax::Binary(_))
            | ast::Expr::ExprMax(ast::ExprMax::BinaryComprehension(_))
            | ast::Expr::ExprMax(ast::ExprMax::BlockExpr(_))
            | ast::Expr::ExprMax(ast::ExprMax::CaseExpr(_))
            | ast::Expr::ExprMax(ast::ExprMax::Char(_))
            | ast::Expr::ExprMax(ast::ExprMax::Float(_))
            | ast::Expr::ExprMax(ast::ExprMax::IfExpr(_))
            | ast::Expr::ExprMax(ast::ExprMax::Integer(_))
            | ast::Expr::ExprMax(ast::ExprMax::List(_))
            | ast::Expr::ExprMax(ast::ExprMax::ListComprehension(_))
            | ast::Expr::ExprMax(ast::ExprMax::MacroCallExpr(_))
            | ast::Expr::ExprMax(ast::ExprMax::MacroString(_))
            | ast::Expr::ExprMax(ast::ExprMax::ParenExpr(_))
            | ast::Expr::ExprMax(ast::ExprMax::ReceiveExpr(_))
            | ast::Expr::ExprMax(ast::ExprMax::String(_))
            | ast::Expr::ExprMax(ast::ExprMax::TryExpr(_))
            | ast::Expr::ExprMax(ast::ExprMax::Tuple(_))
            | ast::Expr::ExprMax(ast::ExprMax::Var(_))
            | ast::Expr::Call(_)
            | ast::Expr::MapExpr(_)
            | ast::Expr::MapExprUpdate(_)
            | ast::Expr::RecordExpr(_)
            | ast::Expr::RecordFieldExpr(_)
            | ast::Expr::RecordIndexExpr(_)
            | ast::Expr::RecordUpdateExpr(_),
    );

    let parent = var.syntax().parent()?;
    let parent_not_needed = match_ast! {
        match parent {
            ast::ExprArgs(_) => true,
            ast::Pipe(_) => true,
            ast::ClauseBody(_) => true,
            ast::CatchExpr(_) => true,
            ast::MatchExpr(_) => true,
            ast::BlockExpr(_) => true,
            ast::CrClause(_) => true,
            _ => false
        }
    };

    Some((
        var.syntax().text_range(),
        !(rhs_not_needed || parent_not_needed),
    ))
}

pub(crate) fn change_indent(delta_indent: i8, str: String) -> String {
    let indent_str = " ".repeat(delta_indent.abs() as usize);
    if str.contains("\n") {
        // Only change indentation if the new string has more than one line.
        str.split("\n")
            .enumerate()
            .map(|(idx, s)| {
                if idx == 0 && s != "" {
                    // No leading newline, trim leading whitespace
                    s.trim_start().to_string()
                } else {
                    if delta_indent >= 0 {
                        if s != "" {
                            format!("{}{}", indent_str, s)
                        } else {
                            s.to_owned()
                        }
                    } else {
                        if let Some(s) = s.strip_prefix(indent_str.as_str()) {
                            s.to_string()
                        } else {
                            // Do not lose useful characters, but remove all leading whitespace
                            s.trim_start().to_string()
                        }
                    }
                }
            })
            .map(|s| s.trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        str.trim_start().to_string()
    }
}

pub const DEFAULT_INDENT_STEP: i8 = 4;

/// Any parameters to the `Clause` that are just a single variable.
pub(crate) fn simple_param_vars(clause: &InFunctionBody<&Clause>) -> Option<FxHashSet<Var>> {
    let mut acc = FxHashSet::default();
    clause.value.pats.iter().for_each(|p| match &clause[*p] {
        hir::Pat::Var(v) => {
            acc.insert(v.clone());
        }
        _ => {}
    });
    Some(acc)
}

#[derive(Debug)]
pub(crate) struct FunctionRanges {
    pub(crate) function: TextRange,
    pub(crate) spec: Option<TextRange>,
    pub(crate) edoc: Vec<TextRange>,
}

impl FunctionRanges {
    pub(crate) fn delete(&self, builder: &mut SourceChangeBuilder) {
        builder.delete(self.function);
        self.spec.into_iter().for_each(|range| {
            builder.delete(range);
        });
        self.edoc.iter().for_each(|range| {
            builder.delete(*range);
        });
    }
}

pub(crate) fn ranges_for_delete_function(
    ctx: &AssistContext,
    ast_fun: &ast::FunDecl,
) -> Option<FunctionRanges> {
    // Look for a possible spec, and delete it too.
    let function_def = match ctx.classify_offset()? {
        SymbolClass::Definition(SymbolDefinition::Function(fun_def)) => Some(fun_def),
        SymbolClass::Reference { refs, typ: _ } => match refs {
            ReferenceClass::Definition(SymbolDefinition::Function(fun_def)) => Some(fun_def),
            _ => None,
        },
        _ => None,
    }?;

    let def_map = ctx.sema.def_map(ctx.file_id());
    let spec = def_map.get_spec(&function_def.function.name);

    let edoc_comments: Vec<InFileAstPtr<ast::Comment>> = if let Some(file_edoc) =
        ctx.sema.form_edoc_comments(InFileAstPtr::new(
            ctx.file_id(),
            AstPtr::new(&ast::Form::FunDecl(ast_fun.clone())),
        )) {
        file_edoc.comments()
    } else {
        vec![]
    };

    let edoc = edoc_comments
        .iter()
        .filter_map(|c| {
            let comment = ctx.ast_ptr_get(*c)?;
            Some(extend_form_range_for_delete(comment.syntax()))
        })
        .collect();

    let spec_range = spec.map(|spec| {
        let ast_spec = ctx.form_ast(spec.spec.form_id);
        extend_form_range_for_delete(ast_spec.syntax())
    });

    Some(FunctionRanges {
        function: extend_form_range_for_delete(ast_fun.syntax()),
        spec: spec_range,
        edoc,
    })
}

fn extend_form_range_for_delete(syntax: &SyntaxNode) -> TextRange {
    let orig_range = syntax.text_range();
    let start = orig_range.start();
    let end = match skip_trailing_newline(syntax) {
        Some(end) => end.end(),
        None => orig_range.end(),
    };
    // Temporary for  T148094436
    let _pctx = stdx::panic_context::enter(format!("\nextend_form_range_for_delete"));
    TextRange::new(start, end)
}

// ---------------------------------------------------------------------

pub fn add_compile_option<'a>(
    sema: &'a Semantic<'a>,
    file_id: FileId,
    option: &str,
    insert_at: Option<TextSize>,
    builder: &'a mut SourceChangeBuilder,
) -> Option<()> {
    let source = sema.parse(file_id).value;
    let form_list = sema.db.file_form_list(file_id);

    builder.edit_file(file_id);
    if form_list.compile_attributes().count() == 0 {
        new_compile_attribute(&form_list, &source, option, insert_at, builder);
        Some(())
    } else {
        if form_list.compile_attributes().count() == 1 {
            // One existing compile attribute, add the option to it.
            let (_, co) = form_list.compile_attributes().next()?;
            add_to_compile_attribute(co, &source, option, builder)
        } else {
            // Multiple, make a new one
            new_compile_attribute(&form_list, &source, option, insert_at, builder);
            Some(())
        }
    }
}

fn new_compile_attribute<'a>(
    form_list: &FormList,
    source: &SourceFile,
    option: &str,
    insert_at: Option<TextSize>,
    builder: &'a mut SourceChangeBuilder,
) {
    let insert = insert_at.unwrap_or_else(|| {
        if let Some(module_attr) = form_list.module_attribute() {
            let module_attr_range = module_attr.form_id.get(&source).syntax().text_range();
            TextSize::from(module_attr_range.end() + TextSize::from(1))
        } else {
            TextSize::from(0)
        }
    });
    builder.insert(insert, format!("\n-compile([{option}]).\n"))
}

fn add_to_compile_attribute<'a>(
    co: &CompileOption,
    source: &SourceFile,
    option: &str,
    builder: &'a mut SourceChangeBuilder,
) -> Option<()> {
    let export_ast = co.form_id.get(source);
    match &export_ast.options()? {
        ast::Expr::ExprMax(ast::ExprMax::List(e)) => {
            // Skip the trailing "]"
            let mut r = e.syntax().text_range().end();
            r -= TextSize::from(1);
            builder.insert(r, format!(", {option}"));
        }
        ast::Expr::ExprMax(ast::ExprMax::Atom(e)) => {
            let r = e.syntax().text_range();
            builder.replace(r, format!("[{}, {option}]", e.syntax().text()));
        }
        ast::Expr::ExprMax(ast::ExprMax::Tuple(e)) => {
            let r = e.syntax().text_range();
            builder.replace(r, format!("[{}, {option}]", e.syntax().text()));
        }
        _ => return None,
    };
    Some(())
}

// ---------------------------------------------------------------------

pub(crate) struct ExportBuilder<'a> {
    sema: &'a Semantic<'a>,
    file_id: FileId,
    funs: &'a [NameArity],
    // `group_with`: Add `funs` to the same export as this, if found.
    // If it is added to the existing export, the comment is not used.
    group_with: Option<NameArity>,
    insert_at: Option<TextSize>,
    with_comment: Option<String>,
    builder: &'a mut SourceChangeBuilder,
}

impl<'a> ExportBuilder<'a> {
    pub(crate) fn new(
        sema: &'a Semantic<'a>,
        file_id: FileId,
        funs: &'a [NameArity],
        builder: &'a mut SourceChangeBuilder,
    ) -> ExportBuilder<'a> {
        ExportBuilder {
            sema,
            file_id,
            funs,
            group_with: None,
            insert_at: None,
            with_comment: None,
            builder,
        }
    }

    pub(crate) fn group_with(mut self, name: NameArity) -> ExportBuilder<'a> {
        self.group_with = Some(name);
        self
    }

    pub(crate) fn insert_at(mut self, location: TextSize) -> ExportBuilder<'a> {
        self.insert_at = Some(location);
        self
    }

    pub(crate) fn with_comment(mut self, comment: String) -> ExportBuilder<'a> {
        self.with_comment = Some(comment);
        self
    }

    pub(crate) fn finish(&mut self) {
        let source = self.sema.parse(self.file_id).value;
        let form_list = self.sema.db.file_form_list(self.file_id);
        let export_text = self
            .funs
            .iter()
            .map(|function_name_arity| format!("{function_name_arity}"))
            .collect::<Vec<_>>()
            .join(", ");

        let (insert, text) = if form_list.exports().count() == 0 {
            self.new_export(form_list, source, export_text)
        } else {
            // Top priority: group_with
            if let Some(group_with) = &self.group_with {
                if let Some((insert, text)) = || -> Option<_> {
                    let (_, export) = form_list.exports().find(|(_, e)| {
                        e.entries
                            .clone()
                            .into_iter()
                            .any(|fa| &form_list[fa].name == group_with)
                    })?;
                    add_to_export(export, &source, &export_text)
                }() {
                    (insert, text)
                } else {
                    self.new_export(form_list, source, export_text)
                }
            } else {
                if self.with_comment.is_some() {
                    // Preceding comment for export, always make a fresh one
                    self.new_export(form_list, source, export_text)
                } else {
                    if let Some((insert, text)) = || -> Option<_> {
                        if form_list.exports().count() == 1 {
                            // One existing export, add the function to it.

                            let (_, export) = form_list.exports().next()?;
                            add_to_export(export, &source, &export_text)
                        } else {
                            // Multiple
                            None
                        }
                    }() {
                        (insert, text)
                    } else {
                        // Zero or multiple existing exports, create a fresh one
                        self.new_export(form_list, source, export_text)
                    }
                }
            }
        };

        self.builder.edit_file(self.file_id);
        self.builder.insert(insert, text)
    }

    fn new_export(
        &self,
        form_list: std::sync::Arc<hir::FormList>,
        source: elp_syntax::SourceFile,
        export_text: String,
    ) -> (TextSize, String) {
        let insert = self.insert_at.unwrap_or_else(|| {
            if let Some(module_attr) = form_list.module_attribute() {
                let module_attr_range = module_attr.form_id.get(&source).syntax().text_range();
                TextSize::from(module_attr_range.end() + TextSize::from(1))
            } else {
                TextSize::from(0)
            }
        });
        match &self.with_comment {
            Some(comment) => (
                insert,
                format!("\n%% {comment}\n-export([{export_text}]).\n"),
            ),
            None => (insert, format!("\n-export([{export_text}]).\n")),
        }
    }
}

fn add_to_export(
    export: &hir::Export,
    source: &elp_syntax::SourceFile,
    export_text: &String,
) -> Option<(TextSize, String)> {
    let export_ast = export.form_id.get(source);
    if let Some(fa) = export_ast.funs().last() {
        Some((fa.syntax().text_range().end(), format!(", {export_text}")))
    } else {
        // Empty export list
        let range = find_next_token(export_ast.syntax(), SyntaxKind::ANON_LBRACK)?;
        Some((range.end(), export_text.clone()))
    }
}
