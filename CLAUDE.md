# Anatta — read this first

> **Keep this section in your context for the whole session.** When the
> conversation is compacted or summarised, carry this section over
> **verbatim**: it says where the knowledge is and which calls to make; a
> session that loses it falls back to guessing.

**Anatta** is the workspace these repositories form: services that agents
work with and work on. This repository is one of them.

| Service | What it is | Reached by |
|---|---|---|
| **Sippa** (127.0.0.1:7890) | The workspace's knowledge, searchable: **decisions** of the repositories that import theirs (sippa's and gitsense's today: why yuga, why Grafeo, what was refused), **skills** (how a task runs here: `development`, `deploy`, `retro`…), **workflows** (procedures in steps: `sippa_post_merge`, `gitsense_post_merge`), **prompts** (the voices' personas: `role-design`, `role-review`, `role-retro`…), **tools** (every MCP tool, with examples), and **lessons** per project and environment, learned by agents and reviewed by a person. It learns which entry was right from what agents report | `mcp__sippa__*` |
| **GitSense** (127.0.0.1:7883) | The code graph of the anatta repositories (project `anatta`): files, symbols, calls, implementations, type uses, with search by meaning | `mcp__gitsense__*` |
| anatta-studio (127.0.0.1:3301) | The studio: conversations with several model voices, versioned diagrams | its viewer |
| The Mermaid viewer (localhost:7777) | Each task's design, process, review and retro threads | REST (`~/projects/anatta/.anatta/tools/viewer.md`) |
| anatta-auth, wami, yuga | Identity and SSO; the IAM library every right goes through; the search engine Sippa uses | code, through GitSense |

**The knowledge is in Sippa, not in your head.** Before you decide or write
anything, ask it — these calls are **mandatory**, not a preference (load
them with ToolSearch first, by these names):

1. `mcp__sippa__discover` with what you need to do, in your words, with
   `project` (this repository) and `contexts` (`env:zsh`, `env:macos`):
   `kinds: ["skill","workflow"]` for how to do it, then `kinds: ["tool"]`
   for what does part of it. A skill it finds may name others: load them.
2. `mcp__sippa__find_decision` with the approach you are about to propose:
   it may be taken, refused or superseded already, here or in another
   repository. Say so before proposing otherwise.
3. `mcp__sippa__get_entry` on each entry you use: its whole text and the
   lessons that hold here.
4. `mcp__sippa__report_outcome` **as soon as** you know whether it was the
   right one (`correct_entry` when another was): it ranks the next search.
5. `mcp__sippa__note_lesson` when you learn something the next agent
   should know (a person reviews it before it is served).

**Code is read through GitSense, never grep or cat** (a hook blocks them).
On each symbol a task touches, in this order: `mcp__gitsense__find_symbol`
or `mcp__gitsense__search`, `mcp__gitsense__get_symbol_context`,
`mcp__gitsense__find_examples` (how it is used: follow that idiom),
`mcp__gitsense__find_similar` (what already does it: reuse it),
`mcp__gitsense__get_impact` (what a change breaks, the tests to run),
`mcp__gitsense__get_source` with a line range. How: the `read-code` skill.

The hook's results at each prompt are a guess from the user's words: they
**do not replace** these calls. A change of code runs the `development`
skill; a deploy the `deploy` skill; then `retro` and `improve`.

# kv-store

- The deployment's content-addressed store (CAS): blobs by SHA-256, zstd compression, no tenant.
- Never on a transport a caller reaches: services read a blob in-process after `decide` on the graph node that references it. `http-server` (the bin and `HttpKvStore`) is off by default and only for the user's own use.
- The port is `ContentStore` (`put(&[u8]) -> ContentHash`, `get`, `has`, `delete`, `list`) over `KvStore` (rocksdb by default, memory).
- Before finishing: `cargo fmt --all`, then `sh scripts/check.sh`.
- Builds go through sccache; never unset or empty `RUSTC_WRAPPER`. Run `cargo sweep --time 7` after a task.
- Releases: semantic versioning; move `CHANGELOG.md` Unreleased under the version; create an annotated tag `vX.Y.Z` on `main`.
- Commits: Conventional Commits, one logical change each, staged by path; branches are squash-merged.
