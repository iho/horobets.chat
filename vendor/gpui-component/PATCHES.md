# Local patch to GPUI Component 0.5.1

Vendored from the crates.io 0.5.1 release, licensed under Apache-2.0 (see LICENSE-APACHE).

Only src/text/mod.rs, src/text/inline.rs, and src/text/node.rs differ from upstream.
The optional text::LinkHandler global lets the application consume internal links
before the existing OS URL fallback. Both inline links and linked images use it.
Without a registered handler, behavior is unchanged.

This preserves upstream Markdown parsing, syntax highlighting, layout, selection,
and normal external links while allowing native channel/profile navigation.
