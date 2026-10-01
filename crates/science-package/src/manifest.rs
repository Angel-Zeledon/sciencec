//! `science.toml`: the keys, what each may hold, and what is refused.
//!
//! `package-manager.md` §4.2 is the schema and §8.2 the codes. This module
//! turns a parsed [`crate::toml::Document`] into a [`Manifest`] and reports
//! every problem it finds, not only the first: unlike a TOML syntax error,
//! a wrong key does not stop the next key from being read correctly.
//!
//! # What this build accepts
//!
//! **Decision. §7.1's F0 manifest, exactly: `[package]` with `name`,
//! `version`, `language`, `description`, `license`, `entry`, `modules` and
//! `sidecar`, and `[dependencies]` accepting `path` only.** Every other key
//! §4.2 defines — a `version` or `git` dependency, `[[binary]]`,
//! `[native.*]`, a `sidecar = true` — is *recognised* and refused with
//! `SP0063`, which says the key is designed and not implemented here.
//!
//! **Reason.** A key that is designed is not an unknown key, and `SP0002`'s
//! *"did you mean"* would be a lie about it. A key that is accepted and then
//! ignored is worse than either: `linalg = "0.7.0"` silently building against
//! nothing is the one outcome a manifest must never have.
//!
//! **Cost.** `SP0063` is a code outside §8.2's block (`SP0060`–`SP0065` are
//! from the range §8.2 leaves free), and it will be retired key by key as F1
//! lands. That is the right lifetime for it.

use science_diagnostics::{Code, Diagnostic, FileId, Label, Span, Suggestion};

use crate::toml::{self, Entry, Key, Spanned, Value};

/// The `[package]` keys §4.2 defines, all of which this build reads.
const PACKAGE_KEYS: &[&str] =
    &["name", "version", "language", "description", "license", "entry", "modules", "sidecar"];

/// The keys a dependency's table may carry (§4.2's *"value forms"*).
const DEPENDENCY_KEYS: &[&str] = &["version", "git", "rev", "branch", "tag", "path"];

/// The top-level tables §4.2 defines.
const TABLES: &[&str] = &["package", "dependencies", "binary", "native", "sidecar"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// The file the manifest was registered as, for diagnostics that point
    /// back into it from somewhere else — a cycle, a name mismatch.
    pub file: FileId,
    pub name: String,
    pub name_span: Span,
    pub version: String,
    /// `[package] language`. Recorded, compared with nothing yet (§4.2).
    pub language: Option<String>,
    /// `[package] entry`, relative to the package root (Decision 15 rule 2).
    pub entry: Option<(String, Span)>,
    /// `[package] modules`, relative to the package root; `src` by default.
    pub modules: String,
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    /// The key it is declared under, which is also the module name it
    /// mounts at (Decision 13).
    pub name: String,
    pub name_span: Span,
    /// The `path`, as written, relative to the declaring package's root.
    pub path: String,
    pub path_span: Span,
}

/// Reads a manifest, or reports everything wrong with it.
pub fn parse(file: FileId, text: &str) -> Result<Manifest, Vec<Diagnostic>> {
    let document = toml::parse(file, text).map_err(|d| vec![d])?;
    let mut reader = Reader { diagnostics: Vec::new() };

    for entry in &document.root {
        reader.unknown(&entry.key[0], TABLES, "a top-level key; every key belongs to a table");
    }

    let mut package: Option<(&toml::Section, Vec<&Entry>)> = None;
    let mut dependencies: Vec<Dependency> = Vec::new();
    for section in &document.sections {
        let head = &section.header[0];
        match head.name.as_str() {
            "package" if section.header.len() == 1 && !section.array => {
                package = Some((section, section.entries.iter().collect()));
            }
            "dependencies" if !section.array && section.header.len() == 1 => {
                for entry in &section.entries {
                    if let Some(dependency) = reader.dependency(&entry.key, &entry.value) {
                        dependencies.push(dependency);
                    }
                }
            }
            // `[dependencies.foo]`, the long spelling of `foo = { … }`.
            "dependencies" if !section.array && section.header.len() == 2 => {
                let value = Spanned { value: Value::Table(section.entries.clone()), span: section.span };
                if let Some(dependency) = reader.dependency(&section.header[1..], &value) {
                    dependencies.push(dependency);
                }
            }
            "binary" | "native" | "sidecar" => reader.not_yet(
                section.span,
                &format!("`[{}{}]`", if section.array { "[" } else { "" }, head.name),
                match head.name.as_str() {
                    "binary" => "§4.2's `[[binary]]`: a package builds the one entry Decision 15 names",
                    "native" => "§4.5's native dependencies, which `native-dependencies.md` owns",
                    _ => "§4.5's sidecar table, which `rust-interop.md` owns",
                },
            ),
            _ => reader.unknown(head, TABLES, "a table"),
        }
    }

    let Some((section, entries)) = package else {
        reader.diagnostics.push(
            Diagnostic::error(Code::sp(3), "`science.toml` has no `[package]` table")
                .with_label(Label::primary(Span::new(file, 0, 0), "expected `[package]` with a `name` and a `version`"))
                .with_note("every manifest begins `[package]`, then `name = \"…\"` and `version = \"…\"`"),
        );
        return Err(reader.diagnostics);
    };

    let mut name: Option<(String, Span)> = None;
    let mut version: Option<String> = None;
    let mut language = None;
    let mut entry_file = None;
    let mut modules = None;
    for entry in entries {
        let key = &entry.key[0];
        if entry.key.len() > 1 || !PACKAGE_KEYS.contains(&key.name.as_str()) {
            reader.unknown(key, PACKAGE_KEYS, "a `[package]` key");
            continue;
        }
        if key.name == "sidecar" {
            match &entry.value.value {
                Value::Bool(false) => {}
                Value::Bool(true) => reader.not_yet(
                    entry.value.span,
                    "`sidecar = true`",
                    "§2.7's Rust sidecar, which `rust-interop.md` owns",
                ),
                other => reader.wrong_type(key, &entry.value, other, "a boolean"),
            }
            continue;
        }
        let Some(text) = reader.string(key, &entry.value) else { continue };
        match key.name.as_str() {
            "name" => {
                if reader.module_name(&text, entry.value.span, "a package name") {
                    name = Some((text, entry.value.span));
                }
            }
            "version" => {
                if reader.semantic_version(&text, entry.value.span) {
                    version = Some(text);
                }
            }
            "language" => language = Some(text),
            "entry" => entry_file = Some((text, entry.value.span)),
            "modules" => modules = Some(text),
            _ => {} // `description`, `license`: recorded nowhere yet, and valid.
        }
    }
    let present = |key: &str| section.entries.iter().any(|e| e.key[0].name == key);
    for required in ["name", "version"] {
        if !present(required) {
            reader.diagnostics.push(
                Diagnostic::error(Code::sp(3), format!("`[package]` has no `{required}`"))
                    .with_label(Label::primary(section.span, format!("this table needs a `{required}` key")))
                    .with_note("`name` and `version` are the two keys a manifest cannot leave out (§4.2)"),
            );
        }
    }

    // A dependency declared twice is a TOML duplicate when both are in
    // `[dependencies]`, and is caught here when one is the long spelling.
    for (i, dependency) in dependencies.iter().enumerate() {
        if let Some(first) = dependencies[..i].iter().find(|d| d.name == dependency.name) {
            reader.diagnostics.push(
                Diagnostic::error(Code::sp(1), format!("`science.toml` is not valid TOML: the dependency `{}` is defined twice", dependency.name))
                    .with_label(Label::primary(dependency.name_span, "defined again here"))
                    .with_label(Label::secondary(first.name_span, "first defined here")),
            );
        }
    }

    if !reader.diagnostics.is_empty() {
        return Err(reader.diagnostics);
    }
    let (name, name_span) = name.expect("a missing or invalid name was reported");
    Ok(Manifest {
        file,
        name,
        name_span,
        version: version.expect("a missing or invalid version was reported"),
        language,
        entry: entry_file,
        modules: modules.unwrap_or_else(|| "src".to_string()),
        dependencies,
    })
}

struct Reader {
    diagnostics: Vec<Diagnostic>,
}

impl Reader {
    /// `SP0002`, with the nearest known key as a fix when one is close.
    fn unknown(&mut self, key: &Key, known: &[&str], what: &str) {
        let mut diagnostic = Diagnostic::error(Code::sp(2), format!("unknown key `{}` in `science.toml`", key.name))
            .with_label(Label::primary(key.span, format!("not {what} `package-manager.md` §4.2 defines")));
        if let Some(nearest) = nearest(&key.name, known) {
            diagnostic = diagnostic.with_suggestion(Suggestion {
                span: key.span,
                replacement: nearest.to_string(),
                message: "a key with a similar name exists".to_string(),
            });
        } else {
            diagnostic = diagnostic.with_note(format!("the keys here are {}", list(known)));
        }
        self.diagnostics.push(diagnostic);
    }

    /// `SP0063`: a key §4.2 designs and this build does not implement.
    fn not_yet(&mut self, span: Span, what: &str, owner: &str) {
        self.diagnostics.push(
            Diagnostic::error(Code::sp(63), format!("{what} is designed but not implemented by this `sciencec`"))
                .with_label(Label::primary(span, "not implemented yet"))
                .with_note(format!("this is {owner}"))
                .with_note("this build implements `package-manager.md` §7.1: `[package]`, and `path` dependencies"),
        );
    }

    /// `SP0065`: a known key holding the wrong kind of value.
    fn wrong_type(&mut self, key: &Key, value: &Spanned, found: &Value, wanted: &str) {
        self.diagnostics.push(
            Diagnostic::error(Code::sp(65), format!("`{}` must be {wanted}, and this is {}", key.name, found.kind()))
                .with_label(Label::primary(value.span, format!("expected {wanted}"))),
        );
    }

    fn string(&mut self, key: &Key, value: &Spanned) -> Option<String> {
        match &value.value {
            Value::String(text) => Some(text.clone()),
            other => {
                self.wrong_type(key, value, other, "a string");
                None
            }
        }
    }

    /// `SP0004`: §4.4's rule, `[a-z][a-z0-9]*`, with the normalised name as a
    /// fix when there is one.
    fn module_name(&mut self, name: &str, span: Span, what: &str) -> bool {
        if is_module_name(name) {
            return true;
        }
        let mut diagnostic = Diagnostic::error(Code::sp(4), format!("`{name}` is not a legal {what}"))
            .with_label(Label::primary(span, "not one lowercase word"))
            .with_note(
                "a package name is its root module's name (Decision 13), so it is `[a-z][a-z0-9]*`: \
                 no hyphens, no underscores, no dots, no capitals",
            );
        if let Some(fixed) = normalised(name) {
            diagnostic = diagnostic.with_suggestion(Suggestion {
                span,
                replacement: format!("\"{fixed}\""),
                message: "the same name, made legal".to_string(),
            });
        }
        self.diagnostics.push(diagnostic);
        false
    }

    /// `SP0005`: §8.2 asks that the message say the caret means nothing here.
    fn semantic_version(&mut self, version: &str, span: Span) -> bool {
        if is_semantic_version(version) {
            return true;
        }
        let operator = version.starts_with(['^', '~', '>', '<', '=', '*']);
        let mut diagnostic = Diagnostic::error(Code::sp(5), format!("`{version}` is not a semantic version"))
            .with_label(Label::primary(span, "expected `MAJOR.MINOR.PATCH`"));
        if operator {
            diagnostic = diagnostic.with_note(
                "a version here carries no range operator: the caret means nothing in `science.toml`, \
                 and the plain version already means \"at least\" (§3.1)",
            );
        }
        self.diagnostics.push(diagnostic);
        false
    }

    fn dependency(&mut self, key: &[Key], value: &Spanned) -> Option<Dependency> {
        let name = &key[0];
        if key.len() > 1 {
            self.unknown(&key[1], DEPENDENCY_KEYS, "a dependency key at this depth");
            return None;
        }
        let name_ok = self.module_name(&name.name, name.span, "dependency name");
        let entries: Vec<Entry> = match &value.value {
            // A bare string is `{ version = "…" }` (§4.2).
            Value::String(_) => {
                self.not_yet(value.span, "a `version` dependency", "§3.1's resolver and §5.2's registry, F1 and F2");
                return None;
            }
            Value::Table(entries) => entries.clone(),
            other => {
                self.wrong_type(name, value, other, "a string or a table");
                return None;
            }
        };
        let mut path: Option<(String, Span)> = None;
        let mut sources: Vec<&Key> = Vec::new();
        for entry in &entries {
            let key = &entry.key[0];
            if entry.key.len() > 1 || !DEPENDENCY_KEYS.contains(&key.name.as_str()) {
                self.unknown(key, DEPENDENCY_KEYS, "a dependency key");
                continue;
            }
            if matches!(key.name.as_str(), "version" | "git" | "path") {
                sources.push(key);
            }
            if key.name == "path" {
                if let Some(text) = self.string(key, &entry.value) {
                    path = Some((text, entry.value.span));
                }
            }
        }
        if sources.len() > 1 {
            let mut diagnostic = Diagnostic::error(
                Code::sp(6),
                format!("the dependency `{}` names more than one source", name.name),
            )
            .with_label(Label::primary(sources[1].span, "a second source"))
            .with_label(Label::secondary(sources[0].span, "the first"));
            diagnostic = diagnostic.with_note("exactly one of `version`, `git` or `path` may be present (§4.2)");
            self.diagnostics.push(diagnostic);
            return None;
        }
        match sources.first().map(|k| k.name.as_str()) {
            Some("version") => {
                self.not_yet(sources[0].span, "a `version` dependency", "§3.1's resolver and §5.2's registry, F1 and F2");
                None
            }
            Some("git") => {
                self.not_yet(sources[0].span, "a `git` dependency", "§5.2's git source, F1");
                None
            }
            Some(_) => {
                let (path, path_span) = path?;
                name_ok.then(|| Dependency { name: name.name.clone(), name_span: name.span, path, path_span })
            }
            None => {
                self.diagnostics.push(
                    Diagnostic::error(Code::sp(3), format!("the dependency `{}` names no source", name.name))
                        .with_label(Label::primary(value.span, "expected `path = \"…\"`"))
                        .with_note("exactly one of `version`, `git` or `path` must be present (§4.2)"),
                );
                None
            }
        }
    }
}

/// `[a-z][a-z0-9]*` — `package-manager.md` §4.4 and Decision 16.
pub fn is_module_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some('a'..='z')) && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9'))
}

/// The nearest legal name, for `SP0004`'s fix: lowercased, with every
/// character the rule refuses removed and any leading digits dropped.
pub fn normalised(name: &str) -> Option<String> {
    let lowered: String = name
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .collect();
    let fixed = lowered.trim_start_matches(|c: char| c.is_ascii_digit()).to_string();
    (!fixed.is_empty() && fixed != name).then_some(fixed)
}

/// `MAJOR.MINOR.PATCH`, optionally with a `-pre` and a `+build` suffix.
pub fn is_semantic_version(version: &str) -> bool {
    let core = version.split(['-', '+']).next().unwrap_or("");
    let parts: Vec<&str> = core.split('.').collect();
    let numeric = |p: &str| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) && (p == "0" || !p.starts_with('0'));
    parts.len() == 3 && parts.iter().all(|p| numeric(p)) && {
        let rest = &version[core.len()..];
        rest.is_empty() || rest[1..].chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+')) && rest.len() > 1
    }
}

fn nearest<'k>(name: &str, known: &[&'k str]) -> Option<&'k str> {
    known
        .iter()
        .map(|k| (distance(name, k), *k))
        .filter(|(d, _)| *d <= 2)
        .min_by_key(|(d, _)| *d)
        .map(|(_, k)| k)
}

fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut current = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current.push((previous[j] + cost).min(previous[j + 1] + 1).min(current[j] + 1));
        }
        previous = current;
    }
    previous[b.len()]
}

fn list(keys: &[&str]) -> String {
    keys.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(text: &str) -> Vec<String> {
        match parse(FileId(0), text) {
            Ok(_) => Vec::new(),
            Err(diagnostics) => diagnostics.iter().map(|d| d.code.to_string()).collect(),
        }
    }

    #[test]
    fn a_minimal_manifest_reads_with_its_defaults() {
        let m = parse(FileId(0), "[package]\nname = \"app\"\nversion = \"0.1.0\"\n").unwrap();
        assert_eq!(m.name, "app");
        assert_eq!(m.modules, "src");
        assert!(m.entry.is_none() && m.dependencies.is_empty());
    }

    #[test]
    fn both_spellings_of_a_path_dependency_read() {
        let m = parse(
            FileId(0),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n[dependencies]\nutil = { path = \"../util\" }\n\
             [dependencies.more]\npath = \"../more\"\n",
        )
        .unwrap();
        let names: Vec<_> = m.dependencies.iter().map(|d| (d.name.as_str(), d.path.as_str())).collect();
        assert_eq!(names, [("util", "../util"), ("more", "../more")]);
    }

    #[test]
    fn each_refusal_carries_its_code() {
        assert_eq!(codes("[package]\nname = \"app\"\nversoin = \"0.1.0\"\n"), ["SP0002", "SP0003"]);
        assert_eq!(codes("[package]\nversion = \"0.1.0\"\n"), ["SP0003"]);
        assert_eq!(codes("[package]\nname = \"my-app\"\nversion = \"0.1.0\"\n"), ["SP0004"]);
        assert_eq!(codes("[package]\nname = \"app\"\nversion = \"^0.1\"\n"), ["SP0005"]);
        assert_eq!(
            codes("[package]\nname = \"app\"\nversion = \"0.1.0\"\n[dependencies]\nx = { path = \"a\", version = \"1.0.0\" }\n"),
            ["SP0006"]
        );
        assert_eq!(
            codes("[package]\nname = \"app\"\nversion = \"0.1.0\"\n[dependencies]\nx = \"1.0.0\"\n"),
            ["SP0063"]
        );
        assert_eq!(codes("[package]\nname = 5\nversion = \"0.1.0\"\n"), ["SP0065"]);
    }

    #[test]
    fn the_name_rule_and_its_fix() {
        assert!(is_module_name("spectra") && is_module_name("linalg2"));
        assert!(!is_module_name("Spectra") && !is_module_name("serde_json") && !is_module_name("2d"));
        assert_eq!(normalised("my-App").as_deref(), Some("myapp"));
    }

    #[test]
    fn versions() {
        for good in ["0.1.0", "1.20.3", "1.0.0-alpha.1", "1.0.0+build"] {
            assert!(is_semantic_version(good), "{good}");
        }
        for bad in ["1.0", "^1.0.0", "01.0.0", "1.0.0-", "x.y.z"] {
            assert!(!is_semantic_version(bad), "{bad}");
        }
    }
}
