use std::borrow::Cow;

use oxc_ast::ast::Expression;
use oxc_diagnostics::OxcDiagnostic;
use oxc_macros::declare_oxc_lint;
use oxc_span::{GetSpan, Span};
use phf::{Map, phf_map};

use crate::{context::LintContext, rule::Rule};

fn deprecated_function(deprecated: &str, new: &str, span: Span) -> OxcDiagnostic {
    OxcDiagnostic::warn(format!("{deprecated:?} has been deprecated in favor of {new:?}"))
        .with_label(span)
}

#[derive(Debug, Default, Clone)]
pub struct JestConfig {
    version: String,
}

#[derive(Debug, Default, Clone)]
pub struct NoDeprecatedFunctions(Box<NoDeprecatedFunctionsConfig>);

#[derive(Debug, Default, Clone)]
pub struct NoDeprecatedFunctionsConfig {
    jest: JestConfig,
}

impl std::ops::Deref for NoDeprecatedFunctions {
    type Target = NoDeprecatedFunctionsConfig;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

declare_oxc_lint!(
    /// ### What it does
    ///
    /// Over the years Jest has accrued some debt in the form of functions that have
    /// either been renamed for clarity, or replaced with more powerful APIs.
    ///
    /// This rule can also autofix a number of these deprecations for you.
    /// #### `jest.resetModuleRegistry`
    /// This function was renamed to `resetModules` in Jest 15 and removed in Jest 27.
    ///
    /// #### `jest.addMatchers`
    /// This function was replaced with `expect.extend` in Jest 17 and removed in Jest 27.
    ///
    /// #### `require.requireActual` & `require.requireMock`
    /// These functions were replaced in Jest 21 and removed in Jest 26.
    ///
    /// Originally, the `requireActual` & `requireMock` the `requireActual`&
    /// `requireMock` functions were placed onto the `require` function.
    ///
    /// These functions were later moved onto the `jest` object in order to be easier
    /// for type checkers to handle, and their use via `require` deprecated. Finally,
    /// the release of Jest 26 saw them removed from the `require` function altogether.
    ///
    /// #### `jest.runTimersToTime`
    /// This function was renamed to `advanceTimersByTime` in Jest 22 and removed in Jest 27.
    ///
    /// #### `jest.genMockFromModule`
    /// This function was renamed to `createMockFromModule` in Jest 26, and is scheduled for removal in Jest 30.
    ///
    /// ### Why is this bad?
    ///
    /// While typically these deprecated functions are kept in the codebase for a number
    /// of majors, eventually they are removed completely.
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    /// ```javascript
    /// jest.resetModuleRegistry // since Jest 15
    /// jest.addMatchers // since Jest 17
    /// ```
    NoDeprecatedFunctions,
    jest,
    style,
    fix
);

const DEPRECATED_FUNCTIONS_MAP: Map<&'static str, (usize, &'static str)> = phf_map! {
    "jest.resetModuleRegistry" => (15, "jest.resetModules"),
    "jest.addMatchers" => (17, "expect.extend"),
    "require.requireMock" => (21, "jest.requireMock"),
    "require.requireActual" => (21, "jest.requireMock"),
    "jest.runTimersToTime" => (22, "jest.advanceTimersByTime"),
    "jest.genMockFromModule" => (26, "jest.createMockFromModule"),
};

impl Rule for NoDeprecatedFunctions {
    fn from_configuration(value: serde_json::Value) -> Self {
        let version = value
            .get(0)
            .and_then(|v| v.get("jest"))
            .and_then(|v| v.get("version"))
            .and_then(|v| serde_json::Value::as_str(v))
            // Todo: Fixed Me
            // Currently set the default version to the (maybe) latest, to help to find more problems in
            // the codebase. In the future, the version should come from the cli option or the config files,
            // such as `package.json` or `eslint.config.js`.
            .unwrap_or("29");

        let major: Vec<&str> = version.split('.').collect();

        Self(Box::new(NoDeprecatedFunctionsConfig {
            jest: JestConfig { version: major[0].to_string() },
        }))
    }

    fn run<'a>(&self, node: &oxc_semantic::AstNode<'a>, ctx: &LintContext<'a>) {
        let Some(mem_expr) = node.kind().as_member_expression_kind() else {
            return;
        };
        let mut chain: Vec<Cow<'a, str>> = Vec::new();
        if let Expression::Identifier(ident) = mem_expr.object() {
            chain.push(Cow::Borrowed(ident.name.as_str()));
        }

        if let Some(name) = mem_expr.static_property_name() {
            chain.push(Cow::Borrowed(name.as_str()));
        }

        let node_name = chain.join(".");
        // Todo: read from configuration
        let jest_version_num: usize = self.jest.version.parse().unwrap_or(29);

        if let Some((base_version, replacement)) = DEPRECATED_FUNCTIONS_MAP.get(&node_name) {
            if jest_version_num >= *base_version {
                ctx.diagnostic_with_fix(
                    deprecated_function(&node_name, replacement, mem_expr.span()),
                    |fixer| fixer.replace(mem_expr.span(), *replacement),
                );
            }
        }
    }
}

#[test]
fn tests() {
    use crate::tester::Tester;

    let pass = vec![
        ("jest", Some(serde_json::json!([{ "jest": { "version": "14" } }]))),
        ("require('fs')", Some(serde_json::json!([{ "jest": { "version": "14" } }]))),
        ("jest.resetModuleRegistry", Some(serde_json::json!([{ "jest": { "version": "14" } }]))),
        ("require.requireActual", Some(serde_json::json!([{ "jest": { "version": "17" } }]))),
        ("jest.genMockFromModule", Some(serde_json::json!([{ "jest": { "version": "25" } }]))),
        ("jest.genMockFromModule", Some(serde_json::json!([{ "jest": { "version": "25.1.1" } }]))),
        ("require.requireActual", Some(serde_json::json!([{ "jest": { "version": "17.2" } }]))),
    ];

    let fail = vec![
        ("jest.resetModuleRegistry", None),
        // replace with `jest.resetModules` in Jest 15
        ("jest.resetModuleRegistry", Some(serde_json::json!([{ "jest": { "version": "16" }}]))),
        // replace with `jest.requireMock` in Jest 17.
        ("jest.addMatchers", Some(serde_json::json!([{ "jest": { "version": "18" }}]))),
        // replace with `jest.requireMock` in Jest 21.
        ("require.requireMock", Some(serde_json::json!([{ "jest": { "version": "22" }}]))),
        // replace with `jest.requireActual` in Jest 21.
        ("require.requireActual", Some(serde_json::json!([{ "jest": { "version": "22" }}]))),
        // replace with `jest.advanceTimersByTime` in Jest 22
        ("jest.runTimersToTime", Some(serde_json::json!([{ "jest": { "version": "23" }}]))),
        // replace with `jest.createMockFromModule` in Jest 26
        ("jest.genMockFromModule", Some(serde_json::json!([{ "jest": { "version": "27" }}]))),
    ];

    let fix = vec![
        (
            "jest.resetModuleRegistry()",
            "jest.resetModules()",
            Some(serde_json::json!([{ "jest": { "version": "21" } }])),
        ),
        (
            "jest.addMatchers",
            "expect.extend",
            Some(serde_json::json!([{ "jest": { "version": "24" } }])),
        ),
        (
            "jest.genMockFromModule",
            "jest.createMockFromModule",
            Some(serde_json::json!([{ "jest": { "version": "26" } }])),
        ),
        (
            "jest.genMockFromModule",
            "jest.createMockFromModule",
            Some(serde_json::json!([{ "jest": { "version": "26.0.0-next.11" } }])),
        ),
    ];

    Tester::new(NoDeprecatedFunctions::NAME, NoDeprecatedFunctions::PLUGIN, pass, fail)
        .with_jest_plugin(true)
        .expect_fix(fix)
        .test_and_snapshot();
}
