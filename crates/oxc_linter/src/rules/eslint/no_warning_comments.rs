use cow_utils::CowUtils;
use oxc_diagnostics::OxcDiagnostic;
use oxc_macros::declare_oxc_lint;
use oxc_span::Span;
use schemars::JsonSchema;
use serde_json::Value;

use crate::{context::LintContext, rule::Rule};

#[derive(Debug, Clone, Copy, PartialEq, Eq, JsonSchema, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
/// Controls where configured warning terms are matched within a comment.
pub enum Location {
    /// Match only the first logical word in the comment after optional decorations.
    Start,
    /// Match any word position in the comment.
    Anywhere,
}

impl Default for Location {
    /// Defaults to `Location::Start`, matching ESLint behavior.
    fn default() -> Self {
        Location::Start
    }
}

#[derive(Debug, Clone, JsonSchema, PartialEq, Eq)]
/// Parsed configuration for the `no-warning-comments` rule.
///
/// Mirrors ESLint's options while keeping values normalized for matching.
pub struct NoWarningCommentsConfig {
    /// Lowercased terms that trigger diagnostics when matched.
    terms: Vec<String>,
    /// Leading one-character decorations to strip when `location` is `start`.
    decorations: Vec<String>,
    /// Position strategy used when matching each term.
    location: Location,
}
#[derive(Debug, Default, Clone, JsonSchema, PartialEq, Eq)]
/// Lint rule wrapper containing the resolved `NoWarningCommentsConfig`.
pub struct NoWarningComments(Box<NoWarningCommentsConfig>);

impl Default for NoWarningCommentsConfig {
    /// Uses ESLint-compatible defaults:
    /// `terms = ["todo", "fixme", "xxx"]`, no decorations, and `location = start`.
    fn default() -> Self {
        Self {
            terms: vec!["todo".to_string(), "fixme".to_string(), "xxx".to_string()],
            decorations: vec![],
            location: Location::default(),
        }
    }
}

impl NoWarningCommentsConfig {
    fn parse_string_array(config: &serde_json::Value, key: &str) -> Option<Vec<String>> {
        config
            .get(key)?
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
    }
    /// Builds a rule configuration from JSON rule options.
    ///
    /// Expected shape is an array where index `0` contains an object with:
    /// - `terms: string[]`
    /// - `decoration: string[]`
    /// - `location: "start" | "anywhere"`
    ///
    /// Invalid or missing entries fall back to defaults.
    pub fn new(value: serde_json::Value) -> Self {
        let mut cfg = NoWarningCommentsConfig::default();

        if let Value::Array(arr) = value {
            if let Some(config) = arr.get(0) {
                if let Some(t) = Self::parse_string_array(config, "terms") {
                    if !t.is_empty() {
                        cfg.terms = t;
                    }
                }

                cfg.decorations =
                    Self::parse_string_array(config, "decoration").unwrap_or_default();

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

#[derive(Debug)]
/// A processed representation of a single source comment used by this rule.
///
/// Comment keeps both:
/// - the original extracted text (raw) for diagnostics, and
/// - a normalized token list (words) for matching configured terms.
///
/// # Example
///
/// Normalizing comment text for matching:
///```
/// let cfg = NoWarningCommentsConfig::new(serde_json::json!([{
///     "terms": ["todo"],
///     "decoration": ["!"],
///     "location": "start"
/// }]));
///
/// let words = Comment::from_raw("!TODO: remove legacy path", &cfg);
/// assert_eq!(words, vec!["todo:", "remove", "legacy", "path"]);
/// ```
struct Comment {
    /// Source span of the comment token, used to place diagnostics.
    span: Span,

    /// Comment text slice as extracted from source for this rule.
    raw: String,

    /// Normalized tokens derived from `raw` (lowercased + whitespace-split).
    ///
    /// Matching logic uses this instead of reparsing `raw` every time.
    words: Vec<String>,

    /// Effective rule options used when this `Comment` was created.
    ///
    /// Stored here so matching helpers can use a consistent config snapshot.
    cfg: NoWarningCommentsConfig,
}

// TODO
// - documentation
// - Comment struct:
//   - reword the description [DONE]
//   - For comment struct add code example [DONE]
//   - examples for the implementation code [DONE]
//   - for the new function we need to improve the description, e.g. parameters [DONE]
// - NoWarningComments struct:
//   - reword the description
//   - it doesn't show field information
//   - no docs for parse_string_array
//   - add example code initializing
// - Review comments,e.g, "if term is "todo", it matches "todo", "todo!", "(todo)", etc."
//
// - Linting
// - Run the final code through copilot to see if it offers any further refactoring opportunities.

impl Comment {
    /// Converts raw comment text into normalized words used by term matching.
    ///
    /// This lowercases input and strips leading decorations until a configured
    /// term is encountered.
    pub fn from_raw(raw: &str, cfg: &NoWarningCommentsConfig) -> Vec<String> {
        // lowercase the raw comment for matching
        let comment_text = raw.cow_to_lowercase();
        // strip leading decoration chars up to the first term

        let cleaned_slice =
            Self::trim_decorations_until_terms(&comment_text, &cfg.decorations, &cfg.terms);
        let cleaned = cleaned_slice.to_string();

        cleaned
            .split_whitespace()
            .filter(|w| !w.is_empty())
            .map(|w| cow_utils::CowUtils::cow_to_lowercase(w).into_owned())
            .collect::<Vec<String>>()
    }

    /// Builds a Comment from a parsed comment span and the active rule config.
    ///
    /// This constructor:
    /// 1. Extracts the comment text from source_text using span.
    /// 2. Skips the first two bytes of the token prefix (for example // or /*).
    /// 3. Normalizes the extracted text into words via from_raw.
    /// 4. Stores a cloned snapshot of cfg for later matching.
    ///
    /// Parameters:
    /// - span: byte span of the comment token in source_text.
    /// - source_text: full file source that contains the comment.
    /// - cfg: resolved no-warning-comments options to apply.
    ///
    /// Returns:
    /// A Comment with source location, extracted raw text, normalized words, and config.
    ///
    /// Note:
    /// This assumes span points to a valid comment token and is in bounds.
    pub fn new(span: Span, source_text: &str, cfg: &NoWarningCommentsConfig) -> Self {
        let raw = &source_text[(span.start as usize + 2)..(span.end as usize)];
        let words = Self::from_raw(raw, cfg);
        Self { span, raw: raw.to_string(), words, cfg: cfg.clone() }
    }

    /// Returns `true` when the comment explicitly disables this rule inline.
    pub fn allow(&self) -> bool {
        self.raw.cow_to_lowercase().contains("no-warning-comments")
    }

    /// Emits a diagnostic when the provided term matches this comment.
    pub fn contains_term(&self, term: &str, ctx: &LintContext) {
        if self.word_matches_term(term) {
            let raw = &self.raw;
            ctx.diagnostic(
                OxcDiagnostic::warn(format!("Unexpected '{term}' comment: '{raw}'."))
                    .with_label(self.span),
            );
        }
    }

    /// Checks whether any normalized word matches the provided term according to
    /// the configured `location` strategy.
    fn word_matches_term(&self, term: &str) -> bool {
        let term_lower = term.cow_to_lowercase();
        match self.cfg.location {
            Location::Start => self.words.first().map_or(false, |word| {
                let word_lower = word.cow_to_lowercase();
                let trimmed = word_lower.trim_end_matches(|c: char| !c.is_alphanumeric());
                trimmed == term_lower
            }),
            Location::Anywhere => self.words.iter().any(|word| {
                let word_lower = word.cow_to_lowercase();
                let is_word_alnum = word_lower.chars().all(char::is_alphanumeric);
                if is_word_alnum {
                    word_lower == term_lower
                } else {
                    word_lower.contains(&*term_lower)
                }
            }),
        }
    }

    /// Trims leading decoration characters until the first configured term.
    ///
    /// This behavior is used for `location = start` semantics so leading comment
    /// markers such as `*`, `!`, or custom decorations do not affect the first
    /// term check.
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

/// Checks if any word matches the term, including non-alphanumeric characters.
/// if term is "todo", it matches "todo", "todo!", "(todo)", etc.
/// This is done by checking if the term is a substring of any word.
/// if word is todoMVC and term is todo it will not match.
/// This function is not working correctly so we need to go through the various options.
/// --- "/* eslint one-var: 2 */" ---

// https://eslint.org/docs/latest/rules/no-warning-comments#options
// if location is "start" then ignore decorators, if "anywhere" then do not ignore decorators. If location is not provided then default to "start".
impl Rule for NoWarningComments {
    /// Scans all parsed comments once and reports diagnostics for configured terms.
    fn run_once(&self, ctx: &LintContext) {
        let cfg = self.0.as_ref();

        ctx.semantic().comments().iter().for_each(|comment| {
            let comment = Comment::new(comment.span, ctx.source_text(), cfg);

            if comment.allow() {
                return;
            }

            for term in &cfg.terms {
                comment.contains_term(term, ctx);
            }
        });
    }

    /// Deserializes rule options into `NoWarningCommentsConfig`.
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

#[test]
fn test_init_comment() {
    let source_text = r"
        var x = 10;
        //TODO: Remove this comment
        var y = 15;
        ";
    let span = Span::new(19, 56);
    let cfg = NoWarningCommentsConfig::default();
    let comment = Comment::new(span, source_text, &cfg);
    assert!(comment.words == vec!["//todo:", "remove", "this", "comment"]);
    assert!(comment.raw == "        //TODO: Remove this comment");
    assert!(comment.cfg == NoWarningCommentsConfig::default());
}

#[test]
fn test_allow() {
    let source_text = r#"/*eslint no-warning-comments: [2, { "terms": ["todo", "fixme", "any other term"], "location": "anywhere" }]*/

        	var x = 10;
        	"#;
    let span = Span::new(1, source_text.len() as u32);
    let cfg = NoWarningCommentsConfig::default();
    let comment = Comment::new(span, source_text, &cfg);
    assert!(comment.allow())
}

#[test]
fn test_word_matches_term() {
    let mut source_text = r"//!XXX comment starting with no spaces (start)";
    let mut span = Span::new(0, source_text.len() as u32);
    let cfg_location_anywhere = NoWarningCommentsConfig::new(
        serde_json::json!([{ "terms": ["!xxx"], "location": "anywhere" }]),
    );
    let cfg_location_start = NoWarningCommentsConfig::new(
        serde_json::json!([{ "terms": ["!xxx"], "location": "start" }]),
    );
    let mut comment = Comment::new(span, source_text, &cfg_location_anywhere);
    assert!(comment.word_matches_term("!xxx"));
    comment = Comment::new(span, source_text, &cfg_location_start);
    assert!(comment.word_matches_term("!xxx"));

    source_text = r"//comment starting with no spaces !XXX (anywhere)";
    span = Span::new(0, source_text.len() as u32);
    comment = Comment::new(span, source_text, &cfg_location_anywhere);
    assert!(comment.word_matches_term("!xxx"));
    comment = Comment::new(span, source_text, &cfg_location_start);
    assert!(!comment.word_matches_term("!xxx"));
}

#[test]
fn test_trim_decorations_until_terms() {
    let source_text = r"!TODO comment starting with no spaces (start)";
    let decorations = vec!["!".to_string()];
    let mut terms = vec![];

    assert!(
        Comment::trim_decorations_until_terms(&source_text, &decorations, &terms)
            == "!TODO comment starting with no spaces (start)"
    );

    terms = vec!["TODO".to_string()];
    assert!(
        Comment::trim_decorations_until_terms(&source_text, &decorations, &terms)
            == "TODO comment starting with no spaces (start)"
    );
}
