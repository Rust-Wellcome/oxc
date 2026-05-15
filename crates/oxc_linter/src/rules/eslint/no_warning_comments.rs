use cow_utils::CowUtils;
use oxc_diagnostics::OxcDiagnostic;
use oxc_macros::declare_oxc_lint;
use oxc_span::Span;
use schemars::JsonSchema;
use serde_json::Value;

use crate::{context::LintContext, rule::Rule};

fn parse_string_array(config: &serde_json::Value, key: &str) -> Option<Vec<String>> {
    config
        .get(key)?
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
}

fn no_with_diagnostic(span: Span, term: &str, comment: &str) -> OxcDiagnostic {
    OxcDiagnostic::warn(format!("Unexpected '{term}' comment: '{comment}'.")).with_label(span)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, JsonSchema, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Location {
    Start,
    Anywhere,
}

impl Default for Location {
    fn default() -> Self {
        Location::Start
    }
}

#[derive(Debug, Clone, JsonSchema)]
pub struct NoWarningCommentsConfig {
    terms: Vec<String>,
    decorations: Vec<String>,
    location: Location,
}
#[derive(Debug, Default, Clone, JsonSchema)]
pub struct NoWarningComments(Box<NoWarningCommentsConfig>);

impl Default for NoWarningCommentsConfig {
    fn default() -> Self {
        Self {
            terms: vec!["todo".to_string(), "fixme".to_string(), "xxx".to_string()],
            decorations: vec![],
            location: Location::default(),
        }
    }
}

impl NoWarningCommentsConfig {
    pub fn new(value: serde_json::Value) -> Self {
        let mut cfg = NoWarningCommentsConfig::default();

        if let Value::Array(arr) = value {
            if let Some(config) = arr.get(0) {
                if let Some(t) = parse_string_array(config, "terms") {
                    if !t.is_empty() {
                        cfg.terms = t;
                    }
                }

                cfg.decorations = parse_string_array(config, "decoration").unwrap_or_default();

                if let Some(location_config) = config.get("location") {
                    if let Ok(loc) = serde_json::from_value::<Location>(location_config.clone()) {
                        cfg.location = loc;
                    }
                }
            }
        }

        cfg
    }
}

struct Comment {
    raw: String,
    cleaned: String,
    words: Vec<String>,
}

impl Comment {
    pub fn from_raw(raw: &str, cfg: &NoWarningCommentsConfig) -> Self {
        // lowercase the raw comment for matching
        let comment_text = raw.cow_to_lowercase();
        // strip leading decoration chars up to the first term
        let cleaned_slice =
            trim_decorations_until_terms(&comment_text, &cfg.decorations, &cfg.terms);
        let cleaned = cleaned_slice.to_string();

        let words = cleaned
            .split_whitespace()
            .filter(|w| !w.is_empty())
            .map(|w| cow_utils::CowUtils::cow_to_lowercase(w).into_owned())
            .collect::<Vec<String>>();

        Self { raw: raw.to_string(), cleaned, words }
    }
}

declare_oxc_lint!(
    /// ### What it does
    ///
    /// Disallow specified warning terms in comments
    ///
    /// ### Why is this bad?
    ///
    /// Developers often add comments to code which is not complete or needs review.
    /// Most likely you want to fix or review the code, and then remove the comment,
    /// before you consider the code to be production ready.
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    /// ```js
    /// // TODO: do something
    /// // FIXME: this is not a good idea
    /// ```
    ///
    NoWarningComments,
    eslint,
    nursery, // TODO: change category to `correctness`, `suspicious`, `pedantic`, `perf`, `restriction`, or `style`
             // See <https://oxc.rs/docs/contribute/linter.html#rule-category> for details
    pending,  // TODO: describe fix capabilities. Remove if no fix can be done,
             // keep at 'pending' if you think one could be added but don't know how.
             // Options are 'fix', 'fix_dangerous', 'suggestion', and 'conditional_fix_suggestion'
     config = NoWarningCommentsConfig,
);

fn trim_decorations_until_terms<'a>(
    s: &'a str,
    decorations: &[String],
    terms: &[String],
) -> &'a str {
    if terms.is_empty() {
        return s;
    }
    let mut offset = 0;
    for (i, c) in s.char_indices() {
        if terms.iter().any(|t| s[i..].starts_with(t.as_str())) {
            break;
        }
        let c_str = c.to_string();
        if decorations.iter().any(|d| d == &c_str) {
            offset = i + c.len_utf8();
        } else {
            break;
        }
    }
    &s[offset..]
}

/// Checks if any word matches the term, including non-alphanumeric characters.
/// if term is "todo", it matches "todo", "todo!", "(todo)", etc.
/// This is done by checking if the term is a substring of any word.
/// if word is todoMVC and term is todo it will not match.
/// This function is not working correctly so we need to go through the various options.
/// --- "/* eslint one-var: 2 */" ---
fn any_word_matches_term(words: &[String], term: &str) -> bool {
    let term_lower = term.cow_to_lowercase();
    words.iter().any(|word| {
        let word_lower = word.cow_to_lowercase();
        let is_word_alnum = word_lower.chars().all(char::is_alphanumeric);
        if is_word_alnum { word_lower == term_lower } else { word_lower.contains(&*term_lower) }
    })
}

fn first_word_matches_term(words: &[String], term: &str) -> bool {
    let term_lower = term.cow_to_lowercase();
    words.first().map_or(false, |word| {
        let word_lower = word.cow_to_lowercase();
        let trimmed = word_lower.trim_end_matches(|c: char| !c.is_alphanumeric());
        trimmed == term_lower
    })
}

// https://eslint.org/docs/latest/rules/no-warning-comments#options
// if location is "start" then ignore decorators, if "anywhere" then do not ignore decorators. If location is not provided then default to "start".
impl Rule for NoWarningComments {
    fn run_once(&self, ctx: &LintContext) {
        let cfg = self.0.as_ref();

        ctx.semantic().comments().iter().for_each(|comment| {
            let span = comment.span;

            let raw_comment = &ctx.source_text()[(span.start as usize + 2)..(span.end as usize)];

            if raw_comment.cow_to_lowercase().contains("no-warning-comments") {
                return;
            }

            let comment = Comment::from_raw(raw_comment, cfg);

            for term in &cfg.terms {
                match &cfg.location {
                    Location::Start => {
                        if first_word_matches_term(&comment.words, term) {
                            ctx.diagnostic(no_with_diagnostic(span, term, raw_comment));
                        }
                    }
                    Location::Anywhere => {
                        if any_word_matches_term(&comment.words, term) {
                            ctx.diagnostic(no_with_diagnostic(span, term, raw_comment));
                        }
                    }
                }
            }
        });
    }

    fn from_configuration(value: serde_json::Value) -> Result<Self, serde_json::error::Error> {
        // Read the configuration for term, decoration and location from value and then
        // return NoWarningComments {} struct with the attributes terms, decoration and locations.
        Ok(Self(Box::new(NoWarningCommentsConfig::new(value))))
    }
}

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
