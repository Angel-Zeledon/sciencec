//! The package graph: the root package and every package its `path`
//! dependencies reach.
//!
//! §7.1 accepts `path` dependencies only, so there is nothing to *resolve* in
//! §3.1's sense — no version set, no MVS. What there is to do is a walk:
//! read each manifest a `path` names, check what the walk can check, and
//! hand back the packages in a fixed order.
//!
//! What it checks, each with §8.2's code where §8.2 has one:
//!
//! - a `path` that names no package (`SP0061`);
//! - a dependency declared under a name that is not the package's own
//!   (`SP0062`) — Decision 13 makes the name the root module, so a
//!   dependency cannot be mounted under a name its own manifest disagrees
//!   with;
//! - two packages of one name from two directories (`SP0010`);
//! - a cycle among `path` dependencies (`SP0013`).

use std::path::{Path, PathBuf};

use science_diagnostics::{Code, Diagnostic, FileId, Label};

use crate::manifest::{self, Dependency, Manifest};

/// The manifest's file name. Decision 12.
pub const MANIFEST: &str = "science.toml";

#[derive(Debug, Clone)]
pub struct Package {
    /// The package root: the directory holding `science.toml`. Canonical, so
    /// two spellings of one directory are one package.
    pub root: PathBuf,
    pub manifest: Manifest,
}

impl Package {
    /// The directory mounted at the package's name (Decision 13).
    pub fn modules(&self) -> PathBuf {
        self.root.join(&self.manifest.modules)
    }
}

/// Every package in a build. `packages[0]` is the root; the rest follow in
/// the order a depth-first walk of the manifests first reached them, which
/// is a function of the manifests alone.
#[derive(Debug, Clone)]
pub struct Graph {
    pub packages: Vec<Package>,
}

impl Graph {
    pub fn root(&self) -> &Package {
        &self.packages[0]
    }

    /// The dependencies, without the root.
    pub fn dependencies(&self) -> &[Package] {
        &self.packages[1..]
    }
}

/// The nearest directory at or above `start` that holds a `science.toml`.
///
/// Upward, the way cargo and git look, so `sciencec build` works from any
/// directory inside a package. `start` is made absolute first; a path that
/// cannot be is searched as given.
pub fn find_root(start: &Path) -> Option<PathBuf> {
    let start = std::fs::canonicalize(start).unwrap_or_else(|_| start.to_path_buf());
    start.ancestors().find(|dir| dir.join(MANIFEST).is_file()).map(Path::to_path_buf)
}

/// Reads one manifest and registers it, through `register`, as a source
/// file — so a diagnostic about it renders against it.
pub type Register<'r> = dyn FnMut(&Path, String) -> FileId + 'r;

/// Walks the graph from the package rooted at `root`.
///
/// Every problem found is returned, not only the first; a package whose
/// manifest is broken is not walked into, so nothing is reported twice.
pub fn load(root: &Path, register: &mut Register<'_>) -> Result<Graph, Vec<Diagnostic>> {
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut walk = Walk { packages: Vec::new(), diagnostics: Vec::new(), register };
    let Some(manifest) = walk.read(&root) else {
        return Err(walk.diagnostics);
    };
    match manifest {
        Ok(manifest) => {
            walk.packages.push(Package { root: root.clone(), manifest });
            let mut stack = vec![0];
            walk.visit(0, &mut stack);
        }
        Err(diagnostics) => walk.diagnostics.extend(diagnostics),
    }
    if walk.diagnostics.is_empty() {
        Ok(Graph { packages: walk.packages })
    } else {
        Err(walk.diagnostics)
    }
}

struct Walk<'r, 'a> {
    packages: Vec<Package>,
    diagnostics: Vec<Diagnostic>,
    register: &'r mut Register<'a>,
}

impl Walk<'_, '_> {
    /// `None` when there is no manifest; the caller says why that matters.
    fn read(&mut self, dir: &Path) -> Option<Result<Manifest, Vec<Diagnostic>>> {
        let path = dir.join(MANIFEST);
        let text = std::fs::read_to_string(&path).ok()?;
        let file = (self.register)(&path, text.clone());
        Some(manifest::parse(file, &text))
    }

    fn visit(&mut self, index: usize, stack: &mut Vec<usize>) {
        let dependencies: Vec<Dependency> = self.packages[index].manifest.dependencies.clone();
        let from = self.packages[index].root.clone();
        for dependency in &dependencies {
            let target = from.join(&dependency.path);
            let Some(target) = std::fs::canonicalize(&target).ok().filter(|t| t.join(MANIFEST).is_file()) else {
                let found = if target.is_dir() {
                    "this directory has no `science.toml` in it"
                } else {
                    "nothing exists at this path"
                };
                self.diagnostics.push(
                    Diagnostic::error(
                        Code::sp(61),
                        format!("the dependency `{}` names no package", dependency.name),
                    )
                    .with_label(Label::primary(dependency.path_span, found))
                    .with_note(
                        "a `path` dependency is a directory holding a `science.toml`, \
                         relative to the directory of the manifest that declares it",
                    ),
                );
                continue;
            };

            if let Some(position) = stack.iter().position(|&i| self.packages[i].root == target) {
                let mut chain: Vec<&str> =
                    stack[position..].iter().map(|&i| self.packages[i].manifest.name.as_str()).collect();
                chain.push(&self.packages[stack[position]].manifest.name);
                self.diagnostics.push(
                    Diagnostic::error(Code::sp(13), "a cycle among `path` dependencies")
                        .with_label(Label::primary(dependency.name_span, "this dependency closes the cycle"))
                        .with_note(format!("the cycle is {}", chain.join(" -> ")))
                        .with_note("a package cannot depend on itself, however indirectly; move what both need into a third package"),
                );
                continue;
            }

            if let Some(existing) = self.packages.iter().position(|p| p.root == target) {
                self.check_name(dependency, existing);
                continue;
            }

            let manifest = match self.read(&target) {
                Some(Ok(manifest)) => manifest,
                Some(Err(diagnostics)) => {
                    self.diagnostics.extend(diagnostics);
                    continue;
                }
                None => continue, // Raced with a deletion; `canonicalize` saw it.
            };
            if let Some(other) = self.packages.iter().find(|p| p.manifest.name == manifest.name) {
                self.diagnostics.push(
                    Diagnostic::error(
                        Code::sp(10),
                        format!("two packages named `{}` come from two different directories", manifest.name),
                    )
                    .with_label(Label::primary(dependency.path_span, format!("this is `{}`", target.display())))
                    .with_label(Label::secondary(other.manifest.name_span, format!("and so is `{}`", other.root.display())))
                    .with_note("one name is one root module (Decision 13), so a build can hold only one of them"),
                );
                continue;
            }
            self.packages.push(Package { root: target, manifest });
            let added = self.packages.len() - 1;
            if !self.check_name(dependency, added) {
                continue;
            }
            stack.push(added);
            self.visit(added, stack);
            stack.pop();
        }
    }

    /// `SP0062` when the key a dependency is declared under is not its name.
    fn check_name(&mut self, dependency: &Dependency, package: usize) -> bool {
        let actual = &self.packages[package].manifest;
        if actual.name == dependency.name {
            return true;
        }
        self.diagnostics.push(
            Diagnostic::error(
                Code::sp(62),
                format!(
                    "the dependency `{}` is a package named `{}`",
                    dependency.name, actual.name
                ),
            )
            .with_label(Label::primary(dependency.name_span, format!("declared as `{}`", dependency.name)))
            .with_label(Label::secondary(actual.name_span, format!("but its manifest names it `{}`", actual.name)))
            .with_note("a package's name is its root module (Decision 13), so it is reached by that name and no other"),
        );
        false
    }
}
