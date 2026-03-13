use cow_utils::CowUtils;
use oxc_diagnostics::OxcDiagnostic;
use oxc_macros::declare_oxc_lint;
use oxc_span::Span;
use schemars::JsonSchema;

use crate::{context::LintContext, rule::Rule};

fn no_with_diagnostic(span: Span, term: &str, comment: &str) -> OxcDiagnostic {
    //
    OxcDiagnostic::warn(format!("Unexpected '{term}' comment: '{comment}'.")).with_label(span)
}

#[derive(Debug, Default, Clone, JsonSchema)]
pub struct NoWarningCommentsConfig {
    terms: Option<Vec<String>>,
    decorations: Option<Vec<String>>,
    location: Option<String>,
}
#[derive(Debug, Default, Clone, JsonSchema)]
pub struct NoWarningComments(Box<NoWarningCommentsConfig>);

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
    pending,  // TODO: describe fix capabilities. Remove if no fix can be done,
             // keep at 'pending' if you think one could be added but don't know how.
             // Options are 'fix', 'fix_dangerous', 'suggestion', and 'conditional_fix_suggestion'
     config = NoWarningCommentsConfig,
);

// Refactor this function to make it more Rusty
fn trim_decorations_until_terms<'a>(
    s: &'a str,             // can accept string slice as input; not an owned String
    decorations: &[String], // slice of string slices
    terms: &[String],       // slice of string slices
) -> &'a str {
    // return a slice of the original string (&'a str) without copying
    let mut i = 0;
    let s_len = s.len();
    while i < s_len {
        if terms.is_empty() {
            break;
        }
        if terms.iter().any(|term| s[i..].starts_with(term)) {
            break; // stop if a term matches
        }
        let c = s[i..].chars().next().unwrap();
        if decorations.iter().any(|d| *d == c.to_string()) {
            i += c.len_utf8(); // keep going with the loop
        } else {
            break; // stop if non-decoration
        }
    }
    &s[i..] // new slice from position i
}

/// Checks if any word matches the term, including non-alphanumeric characters.
/// if term is "todo", it matches "todo", "todo!", "(todo)", etc.
/// This is done by checking if the term is a substring of any word.
/// if word is todoMVC and term is todo it will not match.
/// This function is not working correctly so we need to go through the various options.
/// --- "/* eslint one-var: 2 */" ---
fn any_word_matches_term(words: &[String], term: &str) -> bool {
    let term_lower = term.cow_to_lowercase();
    // let is_term_alnum = term_lower.chars().all(|c| c.is_alphanumeric());
    words.iter().any(|word| {
        // term is alphanumeric, word is alphanumeric, check for exact match ; todo && todoMVC => todo == todoMVC
        // term is alphanumeric, word is not alphanumeric, check for contains ; todo && todo! => todo! contains todo

        let word_lower = word.cow_to_lowercase();
        let is_word_alnum = word_lower.chars().all(char::is_alphanumeric);
        if is_word_alnum { word_lower == term_lower } else { word_lower.contains(&*term_lower) }
    })
}

// https://eslint.org/docs/latest/rules/no-warning-comments#options
// if location is "start" then ignore decorators, if "anywhere" then do not ignore decorators. If location is not provided then default to "start".
impl Rule for NoWarningComments {
    fn run_once(&self, ctx: &LintContext) {
        ctx.semantic().comments().iter().for_each(|comment| {
            let span = comment.span;

            let span_pointers: (u32, u32) = (span.start + 2, span.end);

            let raw_comment = ctx
                .source_text()
                .get((span_pointers.0 as usize)..(span_pointers.1 as usize))
                .unwrap();

            let comment_text = raw_comment.cow_to_lowercase();

            // it would be better to strip comments with no-warning-comments
            if comment_text.contains("no-warning-comments") {
                return;
            }

            // If there are no decorations it returns none so we need to match it otherwise it panics
            let cleaned_text = match &self.0.decorations {
                Some(decorations) => {
                    let empty_vec: Vec<String> = Vec::new();
                    trim_decorations_until_terms(
                        &comment_text,
                        decorations,
                        self.0.terms.as_ref().unwrap_or(&empty_vec),
                    )
                }
                None => &comment_text,
            };

            // We have handled ("//!TODO ", Some(serde_json::json!([{ "decoration": ["*"] }])))
            // But it made the following to fail: Some(serde_json::json!([{ "terms": ["[litera|$]"], "location": "anywhere" }])),
            let words = cleaned_text
                .split_whitespace()
                // .split(|c: char| !c.is_alphanumeric())
                .filter(|w| !w.is_empty())
                .map(|w| cow_utils::CowUtils::cow_to_lowercase(w).into_owned())
                .collect::<Vec<String>>();

            // performance might be an issue here with nested loops. Look at refactoring not with regex.

            // if the terms exist in the comment text then report a diagnostic
            // if there are no terms then use default terms
            if let Some(terms) = &self.0.terms {
                for term in terms {
                    if any_word_matches_term(&words, term) {
                        ctx.diagnostic(no_with_diagnostic(span, term, raw_comment));
                    }
                }
            } else {
                let default_terms = vec!["todo", "fixme", "xxx"];
                for term in default_terms {
                    match &self.0.location {
                        Some(location) => {
                            if location == "start" {
                                if words[0] != term.cow_to_lowercase() {}
                            } else if any_word_matches_term(&words, term) {
                                ctx.diagnostic(no_with_diagnostic(span, term, raw_comment));
                            }
                        }
                        None => {
                            if words[0] == term.cow_to_lowercase() {
                                ctx.diagnostic(no_with_diagnostic(span, term, raw_comment));
                            }
                        }
                    }
                }
            }
            // 24/10/25 there is an issue with the location
            // if you do not have location set to anywhere it will use the default of start
            // if the location is start we should start with rather than contains
        });

        // 1. create a copy of the source code x
        // 2. create a copy of the decoration, location and terms. We can get these from the configuration using the from_config function x
        // 3. escape the decoration special characters. Decoration can be a string or an array of strings. x
        //    if it is a line comment, skip // first
        //    if it is a block comment, skip /* first: we are not there yet.
        //    escape each decoration character until you reach a non-decoration character or the term itself, e.g. *todo
        // 4. creates a constant of /\bno-warning-comments\b/u
        // 5. for each of the warning terms it converts the term to a regular expression:
        //   - escape the term special characters
        //   - create a constant of the word boundary which is \\b
        //   - create a variable for the prefix
        //   - if the location is "start" then it sets prefix to the escaped decoration `^[\\s${escapedDecoration}]*`
        //   - tests /^\w/u against the term and if they match sets the prefix to the word boundary
        //   - sets a constant for the suffix by running a test /\w$/u against the term if true sets suffix to word boundary otherwise sets suffix to an empty string
        //   - creates a constant for flag of "iu"
        //   - returns a regular expression with the prefix, term, and suffix passing in the flags
        // 6. creates a constant comments which gets all of the comments from the source code using the ast comments. Gets all of the tree nodes that are comments.
        // 7. for each comment:
        //   - filters out any comments which start with shebang #!
        //   - runs the check comment function.
        // 8. check comment function:
        //   - sets a constant for the comment text which is node.value
        //   - if it is a directive comment e.g. eslint-disable-next-line or selfConfigRegEx it returns early
        //   - creates a constant of the matches containing the warning terms. Which is a list of warning terms.
        //   - for each match:
        //     - takes the comment and splits it by spaces
        //     - adds the comment to the comment that will be displayed
        //     - if the line is longer than 40 characters it will be truncated with an ellipsis
        //     - it will report the message to the context with messageId unexpected comment with the data being the matched term and the comment but if it is too long it is truncated to 40 characters with an ellipsis
    }

    // this could do with a tidy up.
    fn from_configuration(value: serde_json::Value) -> Result<Self, serde_json::error::Error> {
        // Read the configuration for term, decoration and location from _value and then
        // return NoWarningComments {} struct with the attributes terms, decoration and locations.

        // TODO: Create. NoWarningCommentsConfig struct and box it inside the return struct NoWarningCommentsConfig
        // See crates/oxc_linter/src/rules/eslint/default_case.rs.

        let mut cfg = NoWarningCommentsConfig::default();

        if let Some(config) = value.get(0) {
            if let Some(terms_config) = config.get("terms") {
                cfg.terms = terms_config.as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(std::string::ToString::to_string))
                        .collect::<Vec<String>>()
                });
            }

            if let Some(decorations_config) = config.get("decoration") {
                cfg.decorations = decorations_config.as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(std::string::ToString::to_string))
                        .collect::<Vec<String>>()
                });
            }

            if let Some(location_config) = config.get("location") {
                cfg.location = location_config.as_str().map(std::string::ToString::to_string);
            } else {
                // Default to "start" if location is not provided
                cfg.location = Some("start".to_string());
            }
        }

        Ok(Self(Box::new(cfg)))
    }
}

// cargo insta accept
// cargo test -p oxc_linter -- --nocapture no_warning_comments
#[test]
fn test() {
    use crate::tester::Tester;

    let pass = vec![
        ("// any comment", Some(serde_json::json!([{ "terms": ["fixme"] }]))),
        ("// any comment", Some(serde_json::json!([{ "terms": ["fixme", "todo"] }]))),
        ("// any comment", None),
        ("// any comment", Some(serde_json::json!([{ "location": "anywhere" }]))),
        (
            "// any comment with TODO, FIXME or XXX",
            Some(serde_json::json!([{ "location": "start" }])),
        ),
        ("// any comment with TODO, FIXME or XXX", None),
        ("/* any block comment */", Some(serde_json::json!([{ "terms": ["fixme"] }]))),
        ("/* any block comment */", Some(serde_json::json!([{ "terms": ["fixme", "todo"] }]))),
        ("/* any block comment */", None),
        ("/* any block comment */", Some(serde_json::json!([{ "location": "anywhere" }]))),
        (
            "/* any block comment with TODO, FIXME or XXX */",
            Some(serde_json::json!([{ "location": "start" }])),
        ),
        ("/* any block comment with TODO, FIXME or XXX */", None),
        ("/* any block comment with (TODO, FIXME's or XXX!) */", None),
        (
            "// comments containing terms as substrings like TodoMVC",
            Some(serde_json::json!([{ "terms": ["todo"], "location": "anywhere" }])),
        ),
        (
            "// special regex characters don't cause a problem",
            Some(serde_json::json!([{ "terms": ["[aeiou]"], "location": "anywhere" }])),
        ),
        (
            r#"/*eslint no-warning-comments: [2, { "terms": ["todo", "fixme", "any other term"], "location": "anywhere" }]*/

        	var x = 10;
        	"#,
            None,
        ),
        (
            r#"/*eslint no-warning-comments: [2, { "terms": ["todo", "fixme", "any other term"], "location": "anywhere" }]*/

        	var x = 10;
        	"#,
            Some(serde_json::json!([{ "location": "anywhere" }])),
        ),
        ("// foo", Some(serde_json::json!([{ "terms": ["foo-bar"] }]))),
        (
            "/** multi-line block comment with lines starting with
        	TODO
        	FIXME or
        	XXX
        	*/",
            None,
        ),
        ("//!TODO ", Some(serde_json::json!([{ "decoration": ["*"] }]))),
    ];

    let fail = vec![
        ("// fixme", None),
        ("// any fixme", Some(serde_json::json!([{ "location": "anywhere" }]))),
        ("// any fixme", Some(serde_json::json!([{ "terms": ["fixme"], "location": "anywhere" }]))),
        ("// any FIXME", Some(serde_json::json!([{ "terms": ["fixme"], "location": "anywhere" }]))),
        ("// any fIxMe", Some(serde_json::json!([{ "terms": ["fixme"], "location": "anywhere" }]))),
        (
            "/* any fixme */",
            Some(serde_json::json!([{ "terms": ["FIXME"], "location": "anywhere" }])),
        ),
        (
            "/* any FIXME */",
            Some(serde_json::json!([{ "terms": ["FIXME"], "location": "anywhere" }])),
        ),
        (
            "/* any fIxMe */",
            Some(serde_json::json!([{ "terms": ["FIXME"], "location": "anywhere" }])),
        ),
        (
            "// any fixme or todo",
            Some(serde_json::json!([{ "terms": ["fixme", "todo"], "location": "anywhere" }])),
        ),
        (
            "/* any fixme or todo */",
            Some(serde_json::json!([{ "terms": ["fixme", "todo"], "location": "anywhere" }])),
        ),
        ("/* any fixme or todo */", Some(serde_json::json!([{ "location": "anywhere" }]))),
        ("/* fixme and todo */", None),
        ("/* fixme and todo */", Some(serde_json::json!([{ "location": "anywhere" }]))),
        ("/* any fixme */", Some(serde_json::json!([{ "location": "anywhere" }]))),
        ("/* fixme! */", Some(serde_json::json!([{ "terms": ["fixme"] }]))),
        (
            "// regex [litera|$]",
            Some(serde_json::json!([{ "terms": ["[litera|$]"], "location": "anywhere" }])),
        ),
        ("/* eslint one-var: 2 */", Some(serde_json::json!([{ "terms": ["eslint"] }]))),
        (
            "/* eslint one-var: 2 */",
            Some(serde_json::json!([{ "terms": ["one"], "location": "anywhere" }])),
        ),
        (
            "/* any block comment with TODO, FIXME or XXX */",
            Some(serde_json::json!([{ "location": "anywhere" }])),
        ),
        (
            "/* any block comment with (TODO, FIXME's or XXX!) */",
            Some(serde_json::json!([{ "location": "anywhere" }])),
        ),
        (
            "/**
        	 *any block comment
        	*with (TODO, FIXME's or XXX!) **/",
            Some(serde_json::json!([{ "location": "anywhere" }])),
        ),
        (
            "// any comment with TODO, FIXME or XXX",
            Some(serde_json::json!([{ "location": "anywhere" }])),
        ),
        ("// TODO: something small", Some(serde_json::json!([{ "location": "anywhere" }]))),
        (
            "// TODO: something really longer than 40 characters",
            Some(serde_json::json!([{ "location": "anywhere" }])),
        ),
        (
            "/* TODO: something
        	 really longer than 40 characters
        	 and also a new line */",
            Some(serde_json::json!([{ "location": "anywhere" }])),
        ),
        ("// TODO: small", Some(serde_json::json!([{ "location": "anywhere" }]))),
        (
            "// https://github.com/eslint/eslint/pull/13522#discussion_r470293411 TODO",
            Some(serde_json::json!([{ "location": "anywhere" }])),
        ),
        (
            "// Comment ending with term followed by punctuation TODO!",
            Some(serde_json::json!([{ "terms": ["todo"], "location": "anywhere" }])),
        ),
        // // this test is now failing ...
        (
            "// Comment ending with term including punctuation TODO!",
            Some(serde_json::json!([{ "terms": ["todo!"], "location": "anywhere" }])),
        ),
        (
            "// Comment ending with term including punctuation followed by more TODO!!!",
            Some(serde_json::json!([{ "terms": ["todo!"], "location": "anywhere" }])),
        ),
        (
            "// !TODO comment starting with term preceded by punctuation",
            Some(serde_json::json!([{ "terms": ["todo"], "location": "anywhere" }])),
        ),
        (
            "// !TODO comment starting with term including punctuation",
            Some(serde_json::json!([{ "terms": ["!todo"], "location": "anywhere" }])),
        ),
        (
            "// !!!TODO comment starting with term including punctuation preceded by more",
            Some(serde_json::json!([{ "terms": ["!todo"], "location": "anywhere" }])),
        ),
        (
            "// FIX!term ending with punctuation followed word character",
            Some(serde_json::json!([{ "terms": ["FIX!"], "location": "anywhere" }])),
        ),
        (
            "// Term starting with punctuation preceded word character!FIX",
            Some(serde_json::json!([{ "terms": ["!FIX"], "location": "anywhere" }])),
        ),
        (
            "//!XXX comment starting with no spaces (anywhere)",
            Some(serde_json::json!([{ "terms": ["!xxx"], "location": "anywhere" }])),
        ),
        (
            "//!XXX comment starting with no spaces (start)",
            Some(serde_json::json!([{ "terms": ["!xxx"], "location": "start" }])),
        ),
        (
            "/*
        	TODO undecorated multi-line block comment (start)
        	*/",
            Some(serde_json::json!([{ "terms": ["todo"], "location": "start" }])),
        ),
        (
            "///// TODO decorated single-line comment with decoration array
        	 /////",
            Some(
                serde_json::json!([				{ "terms": ["todo"], "location": "start", "decoration": ["*", "/"] },			]),
            ),
        ),
        (
            "///*/*/ TODO decorated single-line comment with multiple decoration characters (start)
         	 /////",
            Some(
                serde_json::json!([				{ "terms": ["todo"], "location": "start", "decoration": ["*", "/"] },			]),
            ),
        ),
        (
            "//**TODO term starts with a decoration character",
            Some(
                serde_json::json!([				{ "terms": ["*todo"], "location": "start", "decoration": ["*"] },			]),
            ),
        ),
    ];

    Tester::new(NoWarningComments::NAME, NoWarningComments::PLUGIN, pass, fail).test_and_snapshot();
}
