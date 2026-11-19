#set par(justify: true)
#set page(
  margin: 40mm,
  header-ascent: 18pt,
  footer-descent: 18pt,
  header: context {
    if counter(page).get().first() > 1 {
      let prior = query(heading.where(level: 1, outlined: true))
        .filter(h => h.location().page() <= here().page())
      let label = if prior.len() > 0 { prior.last().body } else { [] }
      grid(
        columns: (1fr, auto, 1fr),
        gutter: 0pt,
        align: (left, center, right),
        [],
        label,
        [],
      )
    }
  },
  footer: context {
    let page_no = counter(page).display("1")
    if counter(page).get().first() == 1 {
      align(center)[#page_no]
    } else {
      align(right)[#page_no]
    }
  },
)
#set text(font: "Arial", size: 10pt)
#set heading(numbering: "1.1")

#align(center)[
  #text(size: 20pt, weight: "bold")[Rust Development]
  \
  #text(size: 15pt)[Glossary]
]

#align(center)[Updated: 17 November 2026]

#outline(title: [Contents], depth: 2)

= Cargo structure

== Crate

A *crate* is Rust's compilation unit: code rooted at a crate root, such as
`lib.rs` for a library or `main.rs` for a binary, together with its module
tree. A Cargo package can contain multiple crate-producing targets. For
example, the `tek4051-emulator` package declares a library target rooted at
`lib.rs` and a native binary target rooted at `main.rs`; the package groups
these targets but is not itself a crate.

== Manifest

A *manifest* is a configuration file that describes a project's metadata and
build settings. Cargo uses `Cargo.toml` manifests to define packages,
workspaces, or both. The sections present determine whether the manifest
describes an individual package, a workspace root, or a root package that also
owns a workspace.

== Package

A *package* is a named unit of Rust source and metadata that Cargo builds and
manages. A package's manifest defines its identity and targets; one package
may provide multiple targets, such as a library and one or more binaries. A
package is not the same thing as a crate: targets in a package can produce
separate crates.

== Package manifest

A *package manifest* is a `Cargo.toml` that declares a `[package]` section for
an individual Cargo package. It specifies package metadata and can configure
dependencies, features, and targets. For example,
`dev/src/4051-Emulator/rs/4051-Emulator-rs/Cargo.toml` declares the
`tek4051-emulator` package and its library and native binary targets.

== Workspace

A *workspace* is a set of related packages that Cargo manages together. Its
root coordinates member packages so workspace commands can operate on them
together; workspace members share a lockfile and build output directory. A
workspace may have a root package, or it may use a virtual workspace manifest
with no root package.

== Workspace manifest

A *workspace manifest* is the root `Cargo.toml` containing a `[workspace]`
section. It lists or configures workspace members and may hold settings shared
by those members. If it has no `[package]` section, it is a virtual manifest:
it defines the workspace but is not itself a package. This repository's root
`Cargo.toml` is virtual; it lists the emulator and Flash Drive test packages
and sets resolver version 3.

= Dependency direction

== Downstream

A *downstream crate* or package consumes another crate's public API, usually
by depending on it. The term is relative: a crate can be downstream of its
dependencies and upstream of its own consumers.

== Upstream

An *upstream crate* or package provides an API that another crate consumes,
typically as a dependency. A library is upstream of the packages that depend
on it; those consumers are downstream.

= Modules and visibility

== Module

A *module* is a namespace for organizing Rust items such as functions, types,
and submodules, and for defining their paths and visibility. Modules are
declared with `mod` and can be inline or defined in separate source files;
together they form a hierarchy within a crate.

== Private module

A *private module* is declared without `pub`, so downstream crates cannot
reach it through that module path. Its parent and permitted descendants can
refer to it according to Rust's visibility rules. Making an item inside the
module `pub` does not by itself expose the private module to external crates.

== Submodule

A *submodule* is a module nested within another module. It is part of the
crate's module hierarchy and is addressed through its parent path, such as
`parent::child`. This means a Rust module, not a Git submodule.

= Testing

== Test

A *test* is executable code that checks expected behavior or a property. In
Rust, a test is commonly a function marked `#[test]` and run by `cargo test`.
Integration tests in a package's `tests/` directory exercise its public API as
an external consumer; unit tests can also exercise code within the crate.

== Private test

A *private test* is a test that needs access to private implementation details.
In this repository, keep its body and helpers in a separate file declared by a
private `#[cfg(test)] mod tests;` in the owning module, rather than mixing test
code into the implementation file. As a child module, it can access ancestor
items allowed by Rust's privacy rules. This is a repository test-placement
convention, not a distinct built-in Rust test type.