#![allow(dead_code)]

use lazy_regex::regex;
use oxc_diagnostics::OxcDiagnostic;
use oxc_macros::declare_oxc_lint;
use oxc_span::CompactStr;

use crate::{context::LintContext, rule::Rule};

#[derive(Debug, Default, Clone)]
pub struct NoWarningComments(Box<NoWarningCommentsConfig>);

#[derive(Debug, Default, Clone)]
pub struct NoWarningCommentsConfig {
    terms: Vec<CompactStr>,
}

impl std::ops::Deref for NoWarningComments {
    type Target = NoWarningCommentsConfig;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// See <https://github.com/oxc-project/oxc/issues/6050> for documentation details.
declare_oxc_lint!(
    /// ### What it does
    ///
    /// Briefly describe the rule's purpose.
    ///
    /// ### Why is this bad?
    ///
    /// Explain why violating this rule is problematic.
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    /// ```js
    /// FIXME: Tests will fail if examples are missing or syntactically incorrect.
    /// ```
    ///
    /// Examples of **correct** code for this rule:
    /// ```js
    /// FIXME: Tests will fail if examples are missing or syntactically incorrect.
    /// ```
    NoWarningComments,
    eslint,
    nursery, // TODO: change category to `correctness`, `suspicious`, `pedantic`, `perf`, `restriction`, or `style`
             // See <https://oxc.rs/docs/contribute/linter.html#rule-category> for details
    pending  // TODO: describe fix capabilities. Remove if no fix can be done,
             // keep at 'pending' if you think one could be added but don't know how.
             // Options are 'fix', 'fix_dangerous', 'suggestion', and 'conditional_fix_suggestion'
);

/// <https://github.com/eslint/eslint/blob/main/lib/rules/no-warning-comments.js#L84>
fn convert_to_regexp(term: &str) -> regex::Regex {
    // Decorators are hard-coded here. Read them from config.
    let escaped_decoration = regex::escape(&["*", "/"].join(""));
    let escaped = regex::escape(term);
    let word_boundary = "\\b";

    // "location": optional string that configures where in your comments to
    // check for matches. Defaults to "start".
    // The start is from the first non-decorative character, ignoring whitespace,
    // new lines and characters specified in decoration.
    // The other value is match anywhere in comments.
    // TODO: We need to check the location (from config) here and assign the prefix conditionally. I've omitted it here for now.

    let prefix = format!("^[\\s{escaped_decoration}]*");
    // The regex crate does not support inline flags like /u, so we use RegexBuilder below.
    let re = regex::RegexBuilder::new(r"/\\w$/").unicode(true).build().unwrap();
    let suffix = if re.is_match(term) { word_boundary } else { "" };
    regex::RegexBuilder::new(&format!("{prefix}{escaped}{suffix}"))
        .case_insensitive(true) // for 'i'
        .unicode(true) // for 'u'
        .build()
        .unwrap()
}

/// <https://github.com/eslint/eslint/blob/main/lib/rules/no-warning-comments.js#L142>
fn comment_contains_warning_term(terms: &[CompactStr], comment: &str) -> Vec<CompactStr> {
    let mut matches: Vec<CompactStr> = vec![];
    for (index, term) in terms.iter().enumerate() {
        let re = convert_to_regexp(term);
        if re.is_match(comment) {
            matches.push(terms[index].clone()); // FIXME: Fix this clone
        }
    }
    matches
}

fn check_comment(ctx: &LintContext, comment: &str, terms: &[CompactStr]) {
    let matches = comment_contains_warning_term(terms, comment);
    for _matched_term in &matches {
        ctx.diagnostic(
            OxcDiagnostic::warn("Warning comments shou`ld be avoided")
                .with_help("Use a command-like statement that tells the user how to fix the issue"),
        );
    }
}

impl Rule for NoWarningComments {
    fn from_configuration(value: serde_json::Value) -> Self {
        // Reading the config { "terms": ["fixme"] }
        // References: crates/oxc_linter/src/rules/eslint/max_lines_per_function.rs and crates/oxc_linter/src/rules/eslint/no_bitwise.rs
        let config = value.get(0);
        Self(Box::new(NoWarningCommentsConfig {
            terms: config
                .and_then(|config| config.get("terms"))
                .and_then(serde_json::Value::as_array)
                .map(|v| {
                    v.iter().filter_map(serde_json::Value::as_str).map(CompactStr::from).collect()
                })
                .unwrap_or(vec![
                    CompactStr::new("FIXME"),
                    CompactStr::new("TODO"),
                    CompactStr::new("xxx"),
                ]),
        }))
    }

    // See <https://github.com/Rust-Wellcome/oxc/blob/rule-implementation-no-warning-comments/crates/oxc_linter/src/rule.rs#L38-L40>
    // We are not using `AstNode`` as the comments aren't attached to the tree.
    // Therefore, we do not need to implement the `run` function.
    // We can hence implement `run_once`` and read the comments using the context.
    fn run_once(&self, ctx: &LintContext) {
        ctx.comments().iter().for_each(|comment| {
            let span = comment.span;
            // Recommended in the docs to use let-else over if-let
            let Some(source_comment) =
                ctx.source_text().get((span.start as usize)..(span.end as usize))
            else {
                return;
            };
            check_comment(ctx, source_comment, &self.terms);
        });
    }
}

#[test]
fn test() {
    use crate::tester::Tester;

    let pass = vec![
        ("// any comment", Some(serde_json::json!([{ "terms": ["fixme"] }]))),
        // ("// any comment", Some(serde_json::json!([{ "terms": ["fixme", "todo"] }]))),
        // ("// any comment", None),
        // ("// any comment", Some(serde_json::json!([{ "location": "anywhere" }]))),
        // (
        //     "// any comment with TODO, FIXME or XXX",
        //     Some(serde_json::json!([{ "location": "start" }])),
        // ),
        // ("// any comment with TODO, FIXME or XXX", None),
        // ("/* any block comment */", Some(serde_json::json!([{ "terms": ["fixme"] }]))),
        // ("/* any block comment */", Some(serde_json::json!([{ "terms": ["fixme", "todo"] }]))),
        // ("/* any block comment */", None),
        // ("/* any block comment */", Some(serde_json::json!([{ "location": "anywhere" }]))),
        // (
        //     "/* any block comment with TODO, FIXME or XXX */",
        //     Some(serde_json::json!([{ "location": "start" }])),
        // ),
        // ("/* any block comment with TODO, FIXME or XXX */", None),
        // ("/* any block comment with (TODO, FIXME's or XXX!) */", None),
        // (
        //     "// comments containing terms as substrings like TodoMVC",
        //     Some(serde_json::json!([{ "terms": ["todo"], "location": "anywhere" }])),
        // ),
        // (
        //     "// special regex characters don't cause a problem",
        //     Some(serde_json::json!([{ "terms": ["[aeiou]"], "location": "anywhere" }])),
        // ),
        // (
        //     r#"/*eslint no-warning-comments: [2, { "terms": ["todo", "fixme", "any other term"], "location": "anywhere" }]*/

        // 	var x = 10;
        // 	"#,
        //     None,
        // ),
        // (
        //     r#"/*eslint no-warning-comments: [2, { "terms": ["todo", "fixme", "any other term"], "location": "anywhere" }]*/

        // 	var x = 10;
        // 	"#,
        //     Some(serde_json::json!([{ "location": "anywhere" }])),
        // ),
        // ("// foo", Some(serde_json::json!([{ "terms": ["foo-bar"] }]))),
        // (
        //     "/** multi-line block comment with lines starting with
        // 	TODO
        // 	FIXME or
        // 	XXX
        // 	*/",
        //     None,
        // ),
        // ("//!TODO ", Some(serde_json::json!([{ "decoration": ["*"] }]))),
    ];

    let fail = vec![
        ("// fixme", None),
        // ("// any fixme", Some(serde_json::json!([{ "location": "anywhere" }]))),
        // ("// any fixme", Some(serde_json::json!([{ "terms": ["fixme"], "location": "anywhere" }]))),
        // ("// any FIXME", Some(serde_json::json!([{ "terms": ["fixme"], "location": "anywhere" }]))),
        // ("// any fIxMe", Some(serde_json::json!([{ "terms": ["fixme"], "location": "anywhere" }]))),
        // (
        //     "/* any fixme */",
        //     Some(serde_json::json!([{ "terms": ["FIXME"], "location": "anywhere" }])),
        // ),
        // (
        //     "/* any FIXME */",
        //     Some(serde_json::json!([{ "terms": ["FIXME"], "location": "anywhere" }])),
        // ),
        // (
        //     "/* any fIxMe */",
        //     Some(serde_json::json!([{ "terms": ["FIXME"], "location": "anywhere" }])),
        // ),
        // (
        //     "// any fixme or todo",
        //     Some(serde_json::json!([{ "terms": ["fixme", "todo"], "location": "anywhere" }])),
        // ),
        // (
        //     "/* any fixme or todo */",
        //     Some(serde_json::json!([{ "terms": ["fixme", "todo"], "location": "anywhere" }])),
        // ),
        // ("/* any fixme or todo */", Some(serde_json::json!([{ "location": "anywhere" }]))),
        // ("/* fixme and todo */", None),
        // ("/* fixme and todo */", Some(serde_json::json!([{ "location": "anywhere" }]))),
        // ("/* any fixme */", Some(serde_json::json!([{ "location": "anywhere" }]))),
        // ("/* fixme! */", Some(serde_json::json!([{ "terms": ["fixme"] }]))),
        // (
        //     "// regex [litera|$]",
        //     Some(serde_json::json!([{ "terms": ["[litera|$]"], "location": "anywhere" }])),
        // ),
        // ("/* eslint one-var: 2 */", Some(serde_json::json!([{ "terms": ["eslint"] }]))),
        // (
        //     "/* eslint one-var: 2 */",
        //     Some(serde_json::json!([{ "terms": ["one"], "location": "anywhere" }])),
        // ),
        // (
        //     "/* any block comment with TODO, FIXME or XXX */",
        //     Some(serde_json::json!([{ "location": "anywhere" }])),
        // ),
        // (
        //     "/* any block comment with (TODO, FIXME's or XXX!) */",
        //     Some(serde_json::json!([{ "location": "anywhere" }])),
        // ),
        // (
        //     "/**
        // 	 *any block comment
        // 	*with (TODO, FIXME's or XXX!) **/",
        //     Some(serde_json::json!([{ "location": "anywhere" }])),
        // ),
        // (
        //     "// any comment with TODO, FIXME or XXX",
        //     Some(serde_json::json!([{ "location": "anywhere" }])),
        // ),
        // ("// TODO: something small", Some(serde_json::json!([{ "location": "anywhere" }]))),
        // (
        //     "// TODO: something really longer than 40 characters",
        //     Some(serde_json::json!([{ "location": "anywhere" }])),
        // ),
        // (
        //     "/* TODO: something
        // 	 really longer than 40 characters
        // 	 and also a new line */",
        //     Some(serde_json::json!([{ "location": "anywhere" }])),
        // ),
        // ("// TODO: small", Some(serde_json::json!([{ "location": "anywhere" }]))),
        // (
        //     "// https://github.com/eslint/eslint/pull/13522#discussion_r470293411 TODO",
        //     Some(serde_json::json!([{ "location": "anywhere" }])),
        // ),
        // (
        //     "// Comment ending with term followed by punctuation TODO!",
        //     Some(serde_json::json!([{ "terms": ["todo"], "location": "anywhere" }])),
        // ),
        // (
        //     "// Comment ending with term including punctuation TODO!",
        //     Some(serde_json::json!([{ "terms": ["todo!"], "location": "anywhere" }])),
        // ),
        // (
        //     "// Comment ending with term including punctuation followed by more TODO!!!",
        //     Some(serde_json::json!([{ "terms": ["todo!"], "location": "anywhere" }])),
        // ),
        // (
        //     "// !TODO comment starting with term preceded by punctuation",
        //     Some(serde_json::json!([{ "terms": ["todo"], "location": "anywhere" }])),
        // ),
        // (
        //     "// !TODO comment starting with term including punctuation",
        //     Some(serde_json::json!([{ "terms": ["!todo"], "location": "anywhere" }])),
        // ),
        // (
        //     "// !!!TODO comment starting with term including punctuation preceded by more",
        //     Some(serde_json::json!([{ "terms": ["!todo"], "location": "anywhere" }])),
        // ),
        // (
        //     "// FIX!term ending with punctuation followed word character",
        //     Some(serde_json::json!([{ "terms": ["FIX!"], "location": "anywhere" }])),
        // ),
        // (
        //     "// Term starting with punctuation preceded word character!FIX",
        //     Some(serde_json::json!([{ "terms": ["!FIX"], "location": "anywhere" }])),
        // ),
        // (
        //     "//!XXX comment starting with no spaces (anywhere)",
        //     Some(serde_json::json!([{ "terms": ["!xxx"], "location": "anywhere" }])),
        // ),
        // (
        //     "//!XXX comment starting with no spaces (start)",
        //     Some(serde_json::json!([{ "terms": ["!xxx"], "location": "start" }])),
        // ),
        // (
        //     "/*
        // 	TODO undecorated multi-line block comment (start)
        // 	*/",
        //     Some(serde_json::json!([{ "terms": ["todo"], "location": "start" }])),
        // ),
        // (
        //     "///// TODO decorated single-line comment with decoration array
        // 	 /////",
        //     Some(
        //         serde_json::json!([				{ "terms": ["todo"], "location": "start", "decoration": ["*", "/"] },			]),
        //     ),
        // ),
        // (=
        //     "///*/*/ TODO decorated single-line comment with multiple decoration characters (start)
        // 	 /////",
        //     Some(
        //         serde_json::json!([				{ "terms": ["todo"], "location": "start", "decoration": ["*", "/"] },			]),
        //     ),
        // ),
        // (
        //     "//**TODO term starts with a decoration character",
        //     Some(
        //         serde_json::json!([				{ "terms": ["*todo"], "location": "start", "decoration": ["*"] },			]),
        //     ),
        // ),
    ];

    Tester::new(NoWarningComments::NAME, NoWarningComments::PLUGIN, pass, fail).test_and_snapshot();
}
