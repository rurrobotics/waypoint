@AGENTS.md

## Notes for Claude Code

- Read `AGENTS.md` (imported above) before changing code. It is the single
  source of truth for architecture, conventions and gotchas; update it when
  they change instead of adding them here.
- Look up gpui APIs in the vendored 0.2.2 sources under
  `~/.cargo/registry/src/*/gpui-0.2.2` before using them.
- After UI changes, run the headless tests and take an XWayland screenshot (see
  "Testing the UI") and look at it before reporting the change as done.
- Commit only when asked, in Conventional Commits, one functional step per
  commit.
