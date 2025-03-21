/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under both the MIT license found in the
 * LICENSE-MIT file in the root directory of this source tree and the Apache
 * License, Version 2.0 found in the LICENSE-APACHE file in the root directory
 * of this source tree.
 */

//! Ability to traverse over the hir ast computing a result

use std::ops::Index;

use crate::body::UnexpandedIndex;
use crate::expr::MaybeExpr;
use crate::Body;
use crate::CRClause;
use crate::CallTarget;
use crate::Clause;
use crate::ComprehensionBuilder;
use crate::ComprehensionExpr;
use crate::Expr;
use crate::ExprId;
use crate::Pat;
use crate::PatId;
use crate::Term;
use crate::TermId;
use crate::TypeExpr;
use crate::TypeExprId;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum On {
    Entry,
    Exit,
}

#[derive(Debug)]
pub struct ExprCallBackCtx {
    pub on: On,
    pub in_macro: Option<ExprId>,
    pub expr_id: ExprId,
    pub expr: Expr,
}

#[derive(Debug)]
pub struct PatCallBackCtx {
    pub on: On,
    pub in_macro: Option<ExprId>,
    pub pat_id: PatId,
    pub pat: Pat,
}

#[derive(Debug)]
pub struct TermCallBackCtx {
    pub on: On,
    pub in_macro: Option<ExprId>,
    pub term_id: TermId,
    pub term: Term,
}

pub type ExprCallBack<'a, T> = &'a mut dyn FnMut(T, ExprCallBackCtx) -> T;
pub type PatCallBack<'a, T> = &'a mut dyn FnMut(T, PatCallBackCtx) -> T;
pub type TermCallBack<'a, T> = &'a mut dyn FnMut(T, TermCallBackCtx) -> T;

fn noop_expr_callback<T>(acc: T, _ctx: ExprCallBackCtx) -> T {
    acc
}
fn noop_pat_callback<T>(acc: T, _ctx: PatCallBackCtx) -> T {
    acc
}
fn noop_term_callback<T>(acc: T, _ctx: TermCallBackCtx) -> T {
    acc
}

pub struct FoldCtx<'a, T> {
    body: &'a FoldBody<'a>,
    strategy: Strategy,
    macro_stack: Vec<ExprId>,
    for_expr: ExprCallBack<'a, T>,
    for_pat: PatCallBack<'a, T>,
    for_term: TermCallBack<'a, T>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Strategy {
    TopDown,
    BottomUp,
    Both,
}

#[derive(Debug)]
pub enum FoldBody<'a> {
    Body(&'a Body),
    UnexpandedIndex(UnexpandedIndex<'a>),
}

impl<'a, T> FoldCtx<'a, T> {
    pub fn fold_expr(
        body: &'a Body,
        strategy: Strategy,
        expr_id: ExprId,
        initial: T,
        for_expr: ExprCallBack<'a, T>,
        for_pat: PatCallBack<'a, T>,
    ) -> T {
        FoldCtx {
            body: &FoldBody::Body(body),
            strategy,
            macro_stack: Vec::default(),
            for_expr,
            for_pat,
            for_term: &mut noop_term_callback,
        }
        .do_fold_expr(expr_id, initial)
    }

    pub fn fold_pat(
        body: &'a Body,
        strategy: Strategy,
        pat_id: PatId,
        initial: T,
        for_expr: ExprCallBack<'a, T>,
        for_pat: PatCallBack<'a, T>,
    ) -> T {
        FoldCtx {
            body: &FoldBody::Body(body),
            strategy,
            macro_stack: Vec::default(),
            for_expr,
            for_pat,
            for_term: &mut noop_term_callback,
        }
        .do_fold_pat(pat_id, initial)
    }

    fn in_macro(&self) -> Option<ExprId> {
        if let Some(expr_id) = self.macro_stack.first() {
            Some(*expr_id)
        } else {
            None
        }
    }

    pub fn fold_expr_foldbody(
        body: &'a FoldBody<'a>,
        strategy: Strategy,
        expr_id: ExprId,
        initial: T,
        for_expr: ExprCallBack<'a, T>,
        for_pat: PatCallBack<'a, T>,
    ) -> T {
        FoldCtx {
            body,
            strategy,
            macro_stack: Vec::default(),
            for_expr,
            for_pat,
            for_term: &mut noop_term_callback,
        }
        .do_fold_expr(expr_id, initial)
    }

    pub fn fold_term(
        body: &'a Body,
        strategy: Strategy,
        term_id: TermId,
        initial: T,
        for_term: TermCallBack<'a, T>,
    ) -> T {
        FoldCtx {
            body: &FoldBody::Body(body),
            strategy,
            macro_stack: Vec::default(),
            for_expr: &mut noop_expr_callback,
            for_pat: &mut noop_pat_callback,
            for_term,
        }
        .do_fold_term(term_id, initial)
    }

    // -----------------------------------------------------------------

    fn do_fold_expr(&mut self, expr_id: ExprId, initial: T) -> T {
        let expr = &self.body[expr_id];
        let ctx = ExprCallBackCtx {
            on: On::Entry,
            in_macro: self.in_macro(),
            expr_id,
            expr: expr.clone(),
        };
        let acc = match self.strategy {
            Strategy::TopDown | Strategy::Both => (self.for_expr)(initial, ctx),
            _ => initial,
        };
        let r = match expr {
            crate::Expr::Missing => acc,
            crate::Expr::Literal(_) => acc,
            crate::Expr::Var(_) => acc,
            crate::Expr::Match { lhs, rhs } => {
                let r = self.do_fold_pat(*lhs, acc);
                self.do_fold_expr(*rhs, r)
            }
            crate::Expr::Tuple { exprs } => self.fold_exprs(exprs, acc),
            crate::Expr::List { exprs, tail } => {
                let r = self.fold_exprs(exprs, acc);
                if let Some(expr_id) = tail {
                    self.do_fold_expr(*expr_id, r)
                } else {
                    r
                }
            }
            crate::Expr::Binary { segs } => segs.iter().fold(acc, |acc, binary_seg| {
                let mut r = self.do_fold_expr(binary_seg.elem, acc);
                if let Some(expr_id) = binary_seg.size {
                    r = self.do_fold_expr(expr_id, r);
                }
                r
            }),
            crate::Expr::UnaryOp { expr, op: _ } => self.do_fold_expr(*expr, acc),
            crate::Expr::BinaryOp { lhs, rhs, op: _ } => {
                let r = self.do_fold_expr(*lhs, acc);
                self.do_fold_expr(*rhs, r)
            }
            crate::Expr::Record { name: _, fields } => fields
                .iter()
                .fold(acc, |acc, (_, field)| self.do_fold_expr(*field, acc)),
            crate::Expr::RecordUpdate {
                expr,
                name: _,
                fields,
            } => {
                let r = self.do_fold_expr(*expr, acc);
                fields
                    .iter()
                    .fold(r, |acc, (_, field)| self.do_fold_expr(*field, acc))
            }
            crate::Expr::RecordIndex { name: _, field: _ } => acc,
            crate::Expr::RecordField {
                expr,
                name: _,
                field: _,
            } => self.do_fold_expr(*expr, acc),
            crate::Expr::Map { fields } => fields.iter().fold(acc, |acc, (k, v)| {
                let r = self.do_fold_expr(*k, acc);
                self.do_fold_expr(*v, r)
            }),
            crate::Expr::MapUpdate { expr, fields } => {
                let r = self.do_fold_expr(*expr, acc);
                fields.iter().fold(r, |acc, (lhs, _op, rhs)| {
                    let r = self.do_fold_expr(*lhs, acc);
                    self.do_fold_expr(*rhs, r)
                })
            }
            crate::Expr::Catch { expr } => self.do_fold_expr(*expr, acc),
            crate::Expr::MacroCall { expansion, args: _ } => {
                self.macro_stack.push(expr_id);
                let r = self.do_fold_expr(*expansion, acc);
                self.macro_stack.pop();
                r
            }
            crate::Expr::Call { target, args } => {
                let r = match target {
                    CallTarget::Local { name } => self.do_fold_expr(*name, acc),
                    CallTarget::Remote { module, name } => {
                        let r = self.do_fold_expr(*module, acc);
                        self.do_fold_expr(*name, r)
                    }
                };
                args.iter().fold(r, |acc, arg| self.do_fold_expr(*arg, acc))
            }
            crate::Expr::Comprehension { builder, exprs } => match builder {
                ComprehensionBuilder::List(expr) => self.fold_comprehension(expr, exprs, acc),
                ComprehensionBuilder::Binary(expr) => self.fold_comprehension(expr, exprs, acc),
                ComprehensionBuilder::Map(key, value) => {
                    let r = self.fold_comprehension(key, exprs, acc);
                    self.fold_comprehension(value, exprs, r)
                }
            },
            crate::Expr::Block { exprs } => exprs
                .iter()
                .fold(acc, |acc, expr_id| self.do_fold_expr(*expr_id, acc)),
            crate::Expr::If { clauses } => clauses.iter().fold(acc, |acc, clause| {
                let r = clause.guards.iter().fold(acc, |acc, exprs| {
                    exprs
                        .iter()
                        .fold(acc, |acc, expr| self.do_fold_expr(*expr, acc))
                });
                clause
                    .exprs
                    .iter()
                    .fold(r, |acc, expr| self.do_fold_expr(*expr, acc))
            }),
            crate::Expr::Case { expr, clauses } => {
                let r = self.do_fold_expr(*expr, acc);
                self.fold_cr_clause(clauses, r)
            }
            crate::Expr::Receive { clauses, after } => {
                let mut r = self.fold_cr_clause(clauses, acc);
                if let Some(after) = after {
                    r = self.do_fold_expr(after.timeout, r);
                    r = self.fold_exprs(&after.exprs, r);
                };
                r
            }
            crate::Expr::Try {
                exprs,
                of_clauses,
                catch_clauses,
                after,
            } => {
                let r = exprs
                    .iter()
                    .fold(acc, |acc, expr| self.do_fold_expr(*expr, acc));
                let mut r = self.fold_cr_clause(of_clauses, r);
                r = catch_clauses.iter().fold(r, |acc, clause| {
                    let mut r = acc;
                    if let Some(pat_id) = clause.class {
                        r = self.do_fold_pat(pat_id, r);
                    }
                    r = self.do_fold_pat(clause.reason, r);
                    if let Some(pat_id) = clause.stack {
                        r = self.do_fold_pat(pat_id, r);
                    }

                    r = clause
                        .guards
                        .iter()
                        .fold(r, |acc, exprs| self.fold_exprs(exprs, acc));
                    clause
                        .exprs
                        .iter()
                        .fold(r, |acc, expr| self.do_fold_expr(*expr, acc))
                });
                after
                    .iter()
                    .fold(r, |acc, expr| self.do_fold_expr(*expr, acc))
            }
            crate::Expr::CaptureFun { target, arity } => {
                let r = match target {
                    CallTarget::Local { name } => self.do_fold_expr(*name, acc),
                    CallTarget::Remote { module, name } => {
                        let r = self.do_fold_expr(*module, acc);
                        self.do_fold_expr(*name, r)
                    }
                };
                self.do_fold_expr(*arity, r)
            }
            crate::Expr::Closure { clauses, name: _ } => clauses.iter().fold(
                acc,
                |acc,
                 Clause {
                     pats,
                     guards,
                     exprs,
                 }| {
                    let mut r = pats
                        .iter()
                        .fold(acc, |acc, pat_id| self.do_fold_pat(*pat_id, acc));
                    r = guards
                        .iter()
                        .fold(r, |acc, exprs| self.fold_exprs(exprs, acc));
                    self.fold_exprs(&exprs, r)
                },
            ),
            Expr::Maybe {
                exprs,
                else_clauses,
            } => {
                let r = exprs.iter().fold(acc, |acc, expr| match expr {
                    MaybeExpr::Cond { lhs, rhs } => {
                        let r = self.do_fold_pat(*lhs, acc);
                        self.do_fold_expr(*rhs, r)
                    }
                    MaybeExpr::Expr(expr) => self.do_fold_expr(*expr, acc),
                });
                self.fold_cr_clause(else_clauses, r)
            }
        };
        match self.strategy {
            Strategy::BottomUp | Strategy::Both => {
                let ctx = ExprCallBackCtx {
                    on: On::Exit,
                    in_macro: self.in_macro(),
                    expr_id,
                    expr: expr.clone(),
                };
                (self.for_expr)(r, ctx)
            }
            _ => r,
        }
    }

    fn do_fold_pat(&mut self, pat_id: PatId, initial: T) -> T {
        let pat = &self.body[pat_id];
        let ctx = PatCallBackCtx {
            on: On::Entry,
            in_macro: self.in_macro(),
            pat_id,
            pat: pat.clone(),
        };
        let acc = match self.strategy {
            Strategy::TopDown | Strategy::Both => (self.for_pat)(initial, ctx),
            _ => initial,
        };
        let r = match &pat {
            crate::Pat::Missing => acc,
            crate::Pat::Literal(_) => acc,
            crate::Pat::Var(_) => acc,
            crate::Pat::Match { lhs, rhs } => {
                let r = self.do_fold_pat(*lhs, acc);
                self.do_fold_pat(*rhs, r)
            }
            crate::Pat::Tuple { pats } => self.fold_pats(pats, acc),
            crate::Pat::List { pats, tail } => {
                let mut r = self.fold_pats(pats, acc);
                if let Some(pat_id) = tail {
                    r = self.do_fold_pat(*pat_id, r);
                };
                r
            }
            crate::Pat::Binary { segs } => segs.iter().fold(acc, |acc, binary_seg| {
                let mut r = self.do_fold_pat(binary_seg.elem, acc);
                if let Some(expr_id) = binary_seg.size {
                    r = self.do_fold_expr(expr_id, r);
                }
                r
            }),
            crate::Pat::UnaryOp { pat, op: _ } => self.do_fold_pat(*pat, acc),
            crate::Pat::BinaryOp { lhs, rhs, op: _ } => {
                let r = self.do_fold_pat(*lhs, acc);
                self.do_fold_pat(*rhs, r)
            }
            crate::Pat::Record { name: _, fields } => fields
                .iter()
                .fold(acc, |acc, (_, field)| self.do_fold_pat(*field, acc)),
            crate::Pat::RecordIndex { name: _, field: _ } => acc,
            crate::Pat::Map { fields } => fields.iter().fold(acc, |acc, (k, v)| {
                let r = self.do_fold_expr(*k, acc);
                self.do_fold_pat(*v, r)
            }),
            crate::Pat::MacroCall { expansion, args } => {
                let r = self.do_fold_pat(*expansion, acc);
                args.iter().fold(r, |acc, arg| self.do_fold_expr(*arg, acc))
            }
        };

        match self.strategy {
            Strategy::BottomUp | Strategy::Both => {
                let ctx = PatCallBackCtx {
                    on: On::Exit,
                    in_macro: self.in_macro(),
                    pat_id,
                    pat: pat.clone(),
                };
                (self.for_pat)(r, ctx)
            }
            _ => r,
        }
    }

    fn fold_exprs(&mut self, exprs: &[ExprId], initial: T) -> T {
        exprs
            .iter()
            .fold(initial, |acc, expr_id| self.do_fold_expr(*expr_id, acc))
    }

    fn fold_pats(&mut self, pats: &[PatId], initial: T) -> T {
        pats.iter()
            .fold(initial, |acc, expr_id| self.do_fold_pat(*expr_id, acc))
    }

    fn fold_cr_clause(&mut self, clauses: &[CRClause], initial: T) -> T {
        clauses.iter().fold(initial, |acc, clause| {
            let mut r = self.do_fold_pat(clause.pat, acc);
            r = clause.guards.iter().fold(r, |acc, exprs| {
                exprs
                    .iter()
                    .fold(acc, |acc, expr| self.do_fold_expr(*expr, acc))
            });
            clause
                .exprs
                .iter()
                .fold(r, |acc, expr| self.do_fold_expr(*expr, acc))
        })
    }

    fn fold_comprehension(&mut self, expr: &ExprId, exprs: &[ComprehensionExpr], initial: T) -> T {
        let r = self.do_fold_expr(*expr, initial);
        exprs
            .iter()
            .fold(r, |acc, comprehension_expr| match comprehension_expr {
                ComprehensionExpr::BinGenerator { pat, expr } => {
                    let r = self.do_fold_pat(*pat, acc);
                    self.do_fold_expr(*expr, r)
                }
                ComprehensionExpr::ListGenerator { pat, expr } => {
                    let r = self.do_fold_pat(*pat, acc);
                    self.do_fold_expr(*expr, r)
                }
                ComprehensionExpr::Expr(expr) => self.do_fold_expr(*expr, acc),
                ComprehensionExpr::MapGenerator { key, value, expr } => {
                    let r = self.do_fold_pat(*key, acc);
                    let r = self.do_fold_pat(*value, r);
                    self.do_fold_expr(*expr, r)
                }
            })
    }

    pub fn do_fold_term(&mut self, term_id: TermId, initial: T) -> T {
        let term = &self.body[term_id];
        let ctx = TermCallBackCtx {
            on: On::Entry,
            in_macro: self.in_macro(),
            term_id,
            term: term.clone(),
        };
        let acc = match self.strategy {
            Strategy::TopDown | Strategy::Both => (self.for_term)(initial, ctx),
            _ => initial,
        };
        let r = match &term {
            crate::Term::Missing => acc,
            crate::Term::Literal(_) => acc,
            crate::Term::Binary(_) => acc, // Limited translation of binaries in terms
            crate::Term::Tuple { exprs } => self.do_fold_terms(exprs, acc),
            crate::Term::List { exprs, tail } => {
                let r = self.do_fold_terms(exprs, acc);
                if let Some(term_id) = tail {
                    self.do_fold_term(*term_id, r)
                } else {
                    r
                }
            }
            crate::Term::Map { fields } => fields.iter().fold(acc, |acc, (k, v)| {
                let r = self.do_fold_term(*k, acc);
                self.do_fold_term(*v, r)
            }),
            crate::Term::CaptureFun {
                module: _,
                name: _,
                arity: _,
            } => acc,
            crate::Term::MacroCall { expansion, args: _ } => {
                let r = self.do_fold_term(*expansion, acc);
                // We ignore the args for now
                r
            }
        };
        match self.strategy {
            Strategy::BottomUp | Strategy::Both => {
                let ctx = TermCallBackCtx {
                    on: On::Exit,
                    in_macro: self.in_macro(),
                    term_id,
                    term: term.clone(),
                };
                (self.for_term)(r, ctx)
            }
            _ => r,
        }
    }

    fn do_fold_terms(&mut self, terms: &[TermId], initial: T) -> T {
        terms
            .iter()
            .fold(initial, |acc, expr_id| self.do_fold_term(*expr_id, acc))
    }
}

// ---------------------------------------------------------------------
// Index impls FoldBody

impl<'a> Index<ExprId> for FoldBody<'a> {
    type Output = Expr;

    fn index(&self, index: ExprId) -> &Self::Output {
        match self {
            FoldBody::Body(body) => body.index(index),
            FoldBody::UnexpandedIndex(body) => body.index(index),
        }
    }
}

impl<'a> Index<PatId> for FoldBody<'a> {
    type Output = Pat;

    fn index(&self, index: PatId) -> &Self::Output {
        match self {
            FoldBody::Body(body) => body.index(index),
            FoldBody::UnexpandedIndex(body) => body.index(index),
        }
    }
}

impl<'a> Index<TypeExprId> for FoldBody<'a> {
    type Output = TypeExpr;

    fn index(&self, index: TypeExprId) -> &Self::Output {
        match self {
            FoldBody::Body(body) => body.index(index),
            FoldBody::UnexpandedIndex(body) => body.index(index),
        }
    }
}

impl<'a> Index<TermId> for FoldBody<'a> {
    type Output = Term;

    fn index(&self, index: TermId) -> &Self::Output {
        match self {
            FoldBody::Body(body) => body.index(index),
            FoldBody::UnexpandedIndex(body) => body.index(index),
        }
    }
}

// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use elp_base_db::fixture::WithFixture;
    use elp_syntax::algo;
    use elp_syntax::ast;
    use elp_syntax::AstNode;
    use expect_test::expect;
    use expect_test::Expect;
    use la_arena::Idx;
    use la_arena::RawIdx;

    use super::FoldBody;
    use crate::body::UnexpandedIndex;
    use crate::expr::ClauseId;
    use crate::fold::FoldCtx;
    use crate::fold::Strategy;
    use crate::sema::WithMacros;
    use crate::test_db::TestDB;
    use crate::AnyExprRef;
    use crate::Atom;
    use crate::Expr;
    use crate::FunctionBody;
    use crate::InFile;
    use crate::Literal;
    use crate::Pat;
    use crate::Semantic;
    use crate::Term;
    use crate::TypeExpr;

    fn to_atom(sema: &Semantic<'_>, ast: InFile<&ast::Atom>) -> Option<Atom> {
        let (body, body_map) = sema.find_body(ast.file_id, ast.value.syntax())?;
        let expr = ast.map(|atom| ast::Expr::from(ast::ExprMax::from(atom.clone())));
        let any_expr_id = body_map.any_id(expr.as_ref())?;
        let atom = match body.get_any(any_expr_id) {
            AnyExprRef::Expr(Expr::Literal(Literal::Atom(atom))) => atom,
            AnyExprRef::Pat(Pat::Literal(Literal::Atom(atom))) => atom,
            AnyExprRef::TypeExpr(TypeExpr::Literal(Literal::Atom(atom))) => atom,
            AnyExprRef::Term(Term::Literal(Literal::Atom(atom))) => atom,
            _ => return None,
        };

        Some(atom.clone())
    }

    #[test]
    fn traverse_expr() {
        let fixture_str = r#"
bar() ->
  begin
    A = B + 3,
    [A|A],
    Y = ~A,
    catch A,
    begin
      A,
      Y = 6
    end,
    A
  end.
"#;

        let (db, file_id, range_or_offset) = TestDB::with_range_or_offset(fixture_str);
        let sema = Semantic::new(&db);
        let offset = match range_or_offset {
            elp_base_db::fixture::RangeOrOffset::Range(_) => panic!(),
            elp_base_db::fixture::RangeOrOffset::Offset(o) => o,
        };
        let in_file = sema.parse(file_id);
        let source_file = in_file.value;
        let ast_var = algo::find_node_at_offset::<ast::Var>(source_file.syntax(), offset).unwrap();

        let (body, body_map) = FunctionBody::function_body_with_source_query(
            &db,
            InFile {
                file_id,
                value: Idx::from_raw(RawIdx::from(0)),
            },
        );

        let expr = ast::Expr::ExprMax(ast::ExprMax::Var(ast_var.clone()));
        let expr_id = body_map
            .expr_id(InFile {
                file_id,
                value: &expr,
            })
            .unwrap();
        let expr = &body.body[expr_id];
        let hir_var = match expr {
            crate::Expr::Var(v) => v,
            _ => panic!(),
        };
        let idx = ClauseId::from_raw(RawIdx::from(0));
        let r: u32 = FoldCtx::fold_expr(
            &body.body,
            Strategy::TopDown,
            body.clauses[idx].exprs[0],
            0,
            &mut |acc, ctx| match ctx.expr {
                crate::Expr::Var(v) => {
                    if &v == hir_var {
                        acc + 1
                    } else {
                        acc
                    }
                }
                _ => acc,
            },
            &mut |acc, ctx| match ctx.pat {
                crate::Pat::Var(v) => {
                    if &v == hir_var {
                        acc + 1
                    } else {
                        acc
                    }
                }
                _ => acc,
            },
        );

        // There are 7 occurrences of the Var "A" in the code example
        expect![[r#"
            7
        "#]]
        .assert_debug_eq(&r);
        expect![[r#"
            Var {
                syntax: VAR@51..52
                  VAR@51..52 "A"
                ,
            }
        "#]]
        .assert_debug_eq(&ast_var);
    }

    #[test]
    fn traverse_term() {
        let fixture_str = r#"
-compile([{f~oo,bar},[baz, {foo}]]).
"#;

        let (db, file_id, range_or_offset) = TestDB::with_range_or_offset(fixture_str);
        let sema = Semantic::new(&db);
        let offset = match range_or_offset {
            elp_base_db::fixture::RangeOrOffset::Range(_) => panic!(),
            elp_base_db::fixture::RangeOrOffset::Offset(o) => o,
        };
        let in_file = sema.parse(file_id);
        let source_file = in_file.value;
        let ast_atom =
            algo::find_node_at_offset::<ast::Atom>(source_file.syntax(), offset).unwrap();
        let hir_atom = to_atom(&sema, InFile::new(file_id, &ast_atom)).unwrap();

        let form_list = sema.db.file_form_list(file_id);
        let (idx, _) = form_list.compile_attributes().next().unwrap();
        let compiler_options = sema.db.compile_body(InFile::new(file_id, idx));
        let r = FoldCtx::fold_term(
            &compiler_options.body,
            Strategy::TopDown,
            compiler_options.value,
            0,
            &mut |acc, ctx| match &ctx.term {
                crate::Term::Literal(Literal::Atom(atom)) => {
                    if atom == &hir_atom {
                        acc + 1
                    } else {
                        acc
                    }
                }
                _ => acc,
            },
        );

        // There are 2 occurrences of the atom 'foo' in the code example
        expect![[r#"
            2
        "#]]
        .assert_debug_eq(&r);
        expect![[r#"
            Atom {
                syntax: ATOM@11..14
                  ATOM@11..14 "foo"
                ,
            }
        "#]]
        .assert_debug_eq(&ast_atom);
    }

    #[track_caller]
    fn check_macros(
        with_macros: WithMacros,
        fixture_str: &str,
        tree_expect: Expect,
        r_expect: Expect,
    ) {
        let (db, file_id, range_or_offset) = TestDB::with_range_or_offset(fixture_str);
        let sema = Semantic::new(&db);
        let offset = match range_or_offset {
            elp_base_db::fixture::RangeOrOffset::Range(_) => panic!(),
            elp_base_db::fixture::RangeOrOffset::Offset(o) => o,
        };
        let in_file = sema.parse(file_id);
        let source_file = in_file.value;
        let ast_atom =
            algo::find_node_at_offset::<ast::Atom>(source_file.syntax(), offset).unwrap();
        let hir_atom = to_atom(&sema, InFile::new(file_id, &ast_atom)).unwrap();

        let form_list = sema.db.file_form_list(file_id);
        let (idx, _) = form_list.functions().next().unwrap();
        let compiler_options = sema.db.function_body(InFile::new(file_id, idx));

        let idx = ClauseId::from_raw(RawIdx::from(0));

        let fold_body = if with_macros == WithMacros::Yes {
            FoldBody::UnexpandedIndex(UnexpandedIndex(&compiler_options.body))
        } else {
            FoldBody::Body(&compiler_options.body)
        };
        let r = FoldCtx::fold_expr_foldbody(
            &fold_body,
            Strategy::TopDown,
            compiler_options.clauses[idx].exprs[0],
            (0, 0),
            &mut |(in_macro, not_in_macro), ctx| match ctx.expr {
                crate::Expr::Literal(Literal::Atom(atom)) => {
                    if atom == hir_atom {
                        if ctx.in_macro.is_some() {
                            (in_macro + 1, not_in_macro)
                        } else {
                            (in_macro, not_in_macro + 1)
                        }
                    } else {
                        (in_macro, not_in_macro)
                    }
                }
                _ => (in_macro, not_in_macro),
            },
            &mut |(in_macro, not_in_macro), ctx| match ctx.pat {
                _ => (in_macro, not_in_macro),
            },
        );
        tree_expect.assert_eq(&compiler_options.tree_print(&db));

        r_expect.assert_debug_eq(&r);
    }

    #[test]
    fn macro_aware() {
        check_macros(
            WithMacros::Yes,
            r#"
             -define(AA(X), {X,foo}).
             bar() ->
               begin %% clause.exprs[0]
                 ?AA(f~oo),
                 {foo}
               end.
            "#,
            expect![[r#"

            Clause {
                pats
                guards
                exprs
                    Expr::Block {
                        Expr::Tuple {
                            Literal(Atom('foo')),
                            Literal(Atom('foo')),
                        },
                        Expr::Tuple {
                            Literal(Atom('foo')),
                        },
                    },
            }.
        "#]],
            expect![[r#"
            (
                2,
                1,
            )
        "#]],
        )
    }

    #[test]
    fn ignore_macros() {
        check_macros(
            WithMacros::No,
            r#"
             -define(AA(X), {X,foo}).
             bar() ->
               begin %% clause.exprs[0]
                 ?AA(f~oo),
                 {foo}
               end.
            "#,
            expect![[r#"

            Clause {
                pats
                guards
                exprs
                    Expr::Block {
                        Expr::Tuple {
                            Literal(Atom('foo')),
                            Literal(Atom('foo')),
                        },
                        Expr::Tuple {
                            Literal(Atom('foo')),
                        },
                    },
            }.
        "#]],
            expect![[r#"
            (
                0,
                3,
            )
        "#]],
        )
    }
}
