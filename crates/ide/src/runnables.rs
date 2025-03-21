/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under both the MIT license found in the
 * LICENSE-MIT file in the root directory of this source tree and the Apache
 * License, Version 2.0 found in the LICENSE-APACHE file in the root directory
 * of this source tree.
 */

use elp_ide_db::elp_base_db::FileId;
use elp_ide_db::RootDatabase;
use elp_project_model::AppName;
use hir::NameArity;
use hir::Semantic;

use crate::common_test;
use crate::NavigationTarget;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Runnable {
    pub nav: NavigationTarget,
    pub kind: RunnableKind,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum RunnableKind {
    Test {
        name: NameArity,
        app_name: AppName,
        suite: String,
        case: String,
        group: common_test::GroupName,
    },
    Suite,
}

impl Runnable {
    pub fn label(&self, _target: Option<String>) -> String {
        match &self.kind {
            RunnableKind::Test { .. } => format!("test"),
            RunnableKind::Suite => format!("test"),
        }
    }
    pub fn id(&self) -> String {
        match &self.kind {
            RunnableKind::Test {
                suite, case, group, ..
            } => {
                let group = group.name();
                format!("{suite} - {group}.{case}")
            }
            RunnableKind::Suite => "".to_string(),
        }
    }
    pub fn regex(&self) -> String {
        match &self.kind {
            RunnableKind::Test {
                app_name,
                suite,
                case,
                group,
                ..
            } => {
                let group = group.name();
                format!("{app_name}:{suite} - {group}.{case}$")
            }
            RunnableKind::Suite => "".to_string(),
        }
    }
    pub fn buck2_args(&self, target: String) -> Vec<String> {
        let mut args = Vec::new();
        match &self.kind {
            RunnableKind::Test { .. } => {
                args.push(target);
                args.push("--".to_string());
                args.push("--regex".to_string());
                args.push(self.regex());
                args.push("--print-passing-details".to_string());
                args.push("--run-disabled".to_string());
            }
            RunnableKind::Suite => {
                args.push(target);
                args.push("--".to_string());
                args.push("--print-passing-details".to_string());
                args.push("--run-disabled".to_string());
            }
        }
        args
    }

    // The Unicode variation selector is appended to the play button to avoid that
    // the play symbol is transformed into an emoji
    pub fn run_title(&self) -> String {
        match &self.kind {
            RunnableKind::Test { group, .. } => match group {
                common_test::GroupName::NoGroup => String::from(format!("▶\u{fe0e} Run Test")),
                common_test::GroupName::Name(name) => {
                    String::from(format!("▶\u{fe0e} Run Test (in {})", name))
                }
            },
            RunnableKind::Suite => String::from(format!("▶\u{fe0e} Run All Tests")),
        }
    }
    pub fn debug_title(&self) -> String {
        match &self.kind {
            RunnableKind::Test { group, .. } => match group {
                common_test::GroupName::NoGroup => String::from(format!("▶\u{fe0e} Debug")),
                common_test::GroupName::Name(name) => {
                    String::from(format!("▶\u{fe0e} Debug (in {})", name))
                }
            },
            RunnableKind::Suite => String::from(format!("▶\u{fe0e} Debug")),
        }
    }
}

// Feature: Run
//
// Shows a popup suggesting to run a test **at the current cursor
// location**. Super useful for repeatedly running just a single test. Do bind this
// to a shortcut!
//
// |===
// | Editor  | Action Name
//
// | VS Code | **ELP: Run**
// |===
pub(crate) fn runnables(db: &RootDatabase, file_id: FileId) -> Vec<Runnable> {
    let sema = Semantic::new(db);
    match common_test::runnables(&sema, file_id) {
        Ok(runnables) => runnables,
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {

    use elp_ide_db::elp_base_db::FileRange;
    use stdx::trim_indent;

    use crate::fixture;

    #[track_caller]
    fn check_runnables(fixture: &str) {
        let (analysis, pos, mut annotations) = fixture::annotations(trim_indent(fixture).as_str());
        let runnables = analysis.runnables(pos.file_id).unwrap();
        let mut actual = Vec::new();
        for runnable in runnables {
            let file_id = runnable.nav.file_id;
            let range = runnable.nav.focus_range.unwrap();
            // Remove all non-ascii character to avoid repeating Unicode variation selectors in every test
            let text = trim_indent(
                runnable
                    .run_title()
                    .replace(|c: char| !c.is_ascii(), "")
                    .as_str(),
            );
            actual.push((FileRange { file_id, range }, text));
        }
        let cmp = |(frange, text): &(FileRange, String)| {
            (frange.file_id, frange.range.start(), text.clone())
        };
        actual.sort_by_key(cmp);
        annotations.sort_by_key(cmp);
        assert_eq!(actual, annotations);
    }

    #[test]
    fn runnables_no_suite() {
        check_runnables(
            r#"
 //- /my_app/src/main.erl
    ~
    -module(main).
    -export([all/]).
    main() ->
      ok.
    "#,
        );
    }

    #[test]
    fn runnables_suite() {
        check_runnables(
            r#"
 //- /my_app/test/my_common_test_SUITE.erl
    ~
    -module(my_common_test_SUITE).
 %% ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Run All Tests
    -export([all/0, groups/0]).
    -export([a/1, b/1, c/1]).
    all() -> [a, b, {group, gc1}].
    groups() -> [{gc1, [], [c, {gc2, [], [d]}]}].
    a(_Config) ->
 %% ^ Run Test
      ok.
    b(_Config) ->
 %% ^ Run Test
      ok.
    c(_Config) ->
 %% ^ Run Test (in gc1)
      ok.
    "#,
        );
    }

    #[test]
    fn runnables_suite_not_exported() {
        check_runnables(
            r#"
 //- /my_app/test/my_common_test_SUITE.erl
    ~
    -module(my_common_test_SUITE).
 %% ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Run All Tests
    -export([all/0, groups/0]).
    -export([a/1]).
    all() -> [a, b].
    a(_Config) ->
 %% ^ Run Test
        ok.
    b(_Config) ->
        ok.
    "#,
        );
    }

    #[test]
    fn runnables_nested_groups() {
        check_runnables(
            r#"
 //- /my_app/test/nested_SUITE.erl
    ~
    -module(nested_SUITE).
 %% ^^^^^^^^^^^^^^^^^^^^^^ Run All Tests

    -export([all/0, groups/0]).
    -export([tc1/1, tc2/1, tc3/1, tc4/1, tc5/1]).
    all() ->
        [tc1, {group, g1}, {group, g2, [], [{sg21, []}]}].

    groups() ->
        [ {g1, [], [tc2, {group, g3}]}
        , {g2, [], [tc3, {group, sg21}]}
        , {g3, [], [tc4]}
        , {sg21, [], [tc5]}
        ].

    tc1(_) ->
 %% ^^^ Run Test
        ok.
    tc2(_) ->
 %% ^^^ Run Test (in g1)
        ok.
    tc3(_) ->
 %% ^^^ Run Test (in g2)
        ok.
    tc4(_) ->
 %% ^^^ Run Test (in g1)
 %% ^^^ Run Test (in g3)
        ok.
    tc5(_) ->
 %% ^^^ Run Test (in g2)
 %% ^^^ Run Test (in sg21)
        ok.
"#,
        );
    }

    #[test]
    fn runnables_suite_recursive_groups() {
        check_runnables(
            r#"
 //- /my_app/test/my_common_test_SUITE.erl
    ~
    -module(my_common_test_SUITE).
 %% ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Run All Tests
    -export([all/0, groups/0]).
    -export([a/1, b/1]).
    all() -> [{group, ga}, {group, gb}].
    groups() -> [{ga, [], [a, {group, gb}]},
                 {gb, [], [{group, ga}]}
                ].
    a(_Config) ->
 %% ^ Run Test (in ga)
 %% ^ Run Test (in ga)
 %% ^ Run Test (in gb)
      ok.
    b(_Config) ->
      ok.
    "#,
        );
    }

    #[test]
    fn runnables_suite_otp_example_1() {
        check_runnables(
            r#"
 //- /my_app/test/otp_SUITE.erl
    ~
    -module(otp_SUITE).
 %% ^^^^^^^^^^^^^^^^^^^ Run All Tests
    -compile(export_all).
    groups() -> [{group1, [parallel], [test1a,test1b]},
                 {group2, [shuffle,sequence], [test2a,test2b,test2c]}].
    all() -> [testcase1, {group,group1}, {testcase,testcase2,[{repeat,10}]}, {group,group2}].
    testcase1(_) -> ok.
 %% ^^^^^^^^^ Run Test
    testcase2(_) -> ok.
 %% ^^^^^^^^^ Run Test
    test1a(_) -> ok.
 %% ^^^^^^ Run Test (in group1)
    test1b(_) -> ok.
 %% ^^^^^^ Run Test (in group1)
    test2a(_) -> ok.
 %% ^^^^^^ Run Test (in group2)
    test2b(_) -> ok.
 %% ^^^^^^ Run Test (in group2)
    test2c(_) -> ok.
 %% ^^^^^^ Run Test (in group2)
    "#,
        );
    }

    #[test]
    fn runnables_suite_otp_example_2() {
        check_runnables(
            r#"
 //- /my_app/test/otp_SUITE.erl
    ~
    -module(otp_SUITE).
 %% ^^^^^^^^^^^^^^^^^^^ Run All Tests
    -compile(export_all).
    groups() -> [{tests1, [], [{tests2, [], [t2a,t2b]}, {tests3, [], [t3a,t3b]}]}].
    all() ->[{group, tests1, default, [{tests2, [parallel]}]},
             {group, tests1, default, [{tests2, [shuffle,{repeat,10}]}]}].
    t2a(_) -> ok.
 %% ^^^ Run Test (in tests1)
 %% ^^^ Run Test (in tests2)
    t2b(_) -> ok.
 %% ^^^ Run Test (in tests1)
 %% ^^^ Run Test (in tests2)
    t3a(_) -> ok.
 %% ^^^ Run Test (in tests1)
 %% ^^^ Run Test (in tests3)
    t3b(_) -> ok.
 %% ^^^ Run Test (in tests1)
 %% ^^^ Run Test (in tests3)
    "#,
        );
    }

    #[test]
    fn runnables_suite_otp_example_3() {
        check_runnables(
            r#"
 //- /my_app/test/otp_SUITE.erl
    ~
    -module(otp_SUITE).
 %% ^^^^^^^^^^^^^^^^^^^ Run All Tests
    -compile(export_all).
    groups() -> [{tests1, [], [{tests2, [], [t2a,t2b]},
                 {tests3, [], [t3a,t3b]}]}].
    all() ->
      [{group, tests1, default, [{tests2, [parallel]},
                                 {tests3, default}]},
       {group, tests1, default, [{tests2, [shuffle,{repeat,10}]},
                                 {tests3, default}]}].
    t2a(_) -> ok.
 %% ^^^ Run Test (in tests1)
 %% ^^^ Run Test (in tests2)
    t2b(_) -> ok.
 %% ^^^ Run Test (in tests1)
 %% ^^^ Run Test (in tests2)
    t3a(_) -> ok.
 %% ^^^ Run Test (in tests1)
 %% ^^^ Run Test (in tests3)
    t3b(_) -> ok.
 %% ^^^ Run Test (in tests1)
 %% ^^^ Run Test (in tests3)
    "#,
        );
    }

    #[test]
    fn runnables_suite_otp_example_4() {
        check_runnables(
            r#"
 //- /my_app/test/otp_SUITE.erl
    ~
    -module(otp_SUITE).
 %% ^^^^^^^^^^^^^^^^^^^ Run All Tests
    -compile(export_all).
    groups() ->
      [{tests1, [], [{group, tests2}]},
       {tests2, [], [{group, tests3}]},
       {tests3, [{repeat,2}], [t3a,t3b,t3c]}].

    all() ->
      [{group, tests1, default,
        [{tests2, default,
          [{tests3, [parallel,{repeat,100}]}]}]}].
    t3a(_) -> ok.
 %% ^^^ Run Test (in tests1)
 %% ^^^ Run Test (in tests2)
 %% ^^^ Run Test (in tests3)
    t3b(_) -> ok.
 %% ^^^ Run Test (in tests1)
 %% ^^^ Run Test (in tests2)
 %% ^^^ Run Test (in tests3)
    t3c(_) -> ok.
 %% ^^^ Run Test (in tests1)
 %% ^^^ Run Test (in tests2)
 %% ^^^ Run Test (in tests3)
    "#,
        );
    }
}
