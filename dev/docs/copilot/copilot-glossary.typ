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
  #text(size: 20pt, weight: "bold")[Software Development]
  \
  #text(size: 15pt)[Glossary]
]

#align(center)[Updated: 17 November 2026]

#outline(title: [Contents], depth: 2)

= Planning and delivery

== Entry condition

An *entry condition* is a precondition that must be met before work in a stage
can begin or be accepted into it. It makes readiness explicit, such as a
required dependency or decision being in place; it does not mean the stage is
complete.

== Exit condition

An *exit condition* is a criterion that must be satisfied for a stage to be
considered complete or ready to leave. It may require deliverables, checks, or
review evidence. The exit condition defines what must be true; a gate evaluates
whether the required evidence satisfies it.

== Gate

A *gate* is a decision point in a development workflow with defined acceptance
criteria. Work passes the gate only when the required evidence satisfies those
criteria; otherwise, the gate remains open, or blocked when a specific
dependency prevents a decision. A gate is not a percentage of work completed,
and later implementation does not automatically pass an earlier gate.

== Gatekeeper

A *gatekeeper* is a person, team, or automated process responsible for
evaluating whether a gate's criteria are met and whether work may proceed. A
gatekeeper should apply the stated criteria and communicate the decision and
supporting evidence.

== Grade

A *grade* is a named stage in a development plan that groups related work
around a goal and defines its exit criteria. A grade is complete only when its
criteria are met with recorded evidence. In an ordered plan, grades are not
percentages, and completing a later grade does not waive an earlier one.

== Plan

A *plan* is an organized description of intended work toward an outcome. It
may define scope, tasks, their dependencies or order, and acceptance criteria
so work can be coordinated and assessed. A plan describes what is intended;
it does not itself prove that the work or its acceptance criteria are complete.

== Stage

A *stage* is a distinct phase or portion of a larger workflow that groups
related work or decisions. A stage may have entry and exit conditions; in a
graded plan, a grade is a named stage with explicit exit criteria. A stage is
the portion of work, while a gate is the decision that checks its criteria.

== Tighten

To *tighten* a requirement, rule, constraint, or check is to make it more
precise or restrictive, reducing ambiguity or narrowing what passes. Tightening
should stay within the requested scope and preserve intended behavior; it does
not by itself authorize unrelated changes.

= Requirements and validation

== Check

A *check* is a verification activity that examines work or behavior against
defined criteria and produces an observable result or evidence. Checks may be
automated, such as a test, build, or formatter, or performed manually, such as
a review. Passing one check establishes only what that check covers.

== Constraint

A *constraint* is a boundary that limits acceptable solutions or work, such as
a platform, compatibility, security, resource, or scope limitation. A solution
must respect applicable constraints even when they are not themselves the
desired outcome.

== Requirement

A *requirement* states an outcome, behavior, quality, or capability that a
system or process must satisfy. A useful requirement is clear enough to guide
implementation and verify; it describes what is needed without necessarily
prescribing how to implement it.

== Rule

A *rule* is a directive that governs what is allowed or how work or decisions
must be handled. Rules establish required or prohibited conduct within their
scope, while requirements describe outcomes to achieve and constraints bound
the acceptable choices.

= Architecture and structure

== Boundary

A *boundary* is a defined separation between responsibilities, components, or
ownership domains. It specifies what each side controls and what interactions
or data may cross, often through an interface or visibility rule. A folder
layout can express a boundary, but does not by itself guarantee that the
separation is meaningful or enforced.

== Component

A *component* is a cohesive unit of a software system with a defined purpose
and interactions with other parts. It may be a module, package, library,
service, or UI area; it does not have to be a separately deployed process.

== Owner

An *owner* is the person, team, or component accountable for maintaining a
resource or responsibility and handling decisions or changes within its
defined scope. In technical design, a component may own the state it controls.

== Ownership

*Ownership* is the assignment of accountability and control for a resource,
decision, responsibility, or state. Clear ownership identifies who can make
changes and who is answerable for maintaining the item; in this architectural
sense, it is distinct from Rust's language-level memory ownership.

== Responsibility

A *responsibility* is a defined duty, behavior, or decision that a role or
component is expected to handle. It describes what must be done or controlled,
not merely where the related code happens to be stored.

= Design principles and patterns

== Anti-pattern

An *anti-pattern* is a recurring approach that may appear to solve a problem
but tends to create worse consequences in a particular context. It is not
simply a technique someone dislikes; assess its effects and context before
applying the label.

== Pattern

A *pattern* is a reusable description of an approach to a recurring problem in
a particular context. It captures a solution's structure and tradeoffs rather
than prescribing code to copy unchanged.

== SOLID

*SOLID* is a mnemonic for five object-oriented design principles:

- *Single Responsibility Principle (SRP):* Keep a unit focused on one cohesive
  responsibility, or one main reason to change. *Does not mean:* one method
  per class or one class for every feature.
- *Open/Closed Principle (OCP):* Make it possible to add needed behavior
  through suitable extension points without repeatedly changing stable code.
  *Does not mean:* existing code can never be modified or every hypothetical
  variation needs an abstraction.
- *Liskov Substitution Principle (LSP):* A subtype should be usable wherever
  its base type is expected without breaking clients' behavioral expectations.
  *Does not mean:* matching method signatures alone guarantees substitutability
  or that inheritance should be used everywhere.
- *Interface Segregation Principle (ISP):* Keep interfaces focused so clients
  need not depend on operations they do not use. *Does not mean:* create an
  interface for every method or split interfaces without a real client need.
- *Dependency Inversion Principle (DIP):* High-level policy and low-level
  details should depend on suitable abstractions, not directly on each other.
  *Does not mean:* every dependency needs an interface or a dependency-injection
  framework.

These are design heuristics, not a checklist or a guarantee of good design.
Apply them when they address a concrete coupling or change problem, and weigh
their costs against the simplicity of the design.