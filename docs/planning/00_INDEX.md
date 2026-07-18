# Lightweight tmux Workflow Layer — Implementation Planning Pack

Working name: `tmx`

This pack captures the shared design decisions for a lightweight replacement for the parts of NTM that are actually valuable in the current workflow: fast discovery and switching across tmux sessions/windows/panes, current-directory session creation, naming/renaming, lightweight notes, mobile-friendly operation over SSH, and eventually named layouts and context reminders.

These documents are historical planning artifacts, preserved for design context. The implementation now lives in `src/`; see the repository README and `docs/BUILD_AND_RUN.md` for current usage.

## Documents

1. `01_SHARED_UNDERSTANDING.md` — agreed constraints, non-goals, and final design choices.
2. `02_PRODUCT_REQUIREMENTS.md` — user-facing requirements and acceptance criteria.
3. `03_ARCHITECTURE.md` — architecture, module boundaries, state model, and tmux integration.
4. `04_UX_AND_COMMANDS.md` — command namespace, palette behavior, desktop/mobile UI, and keybindings.
5. `05_IMPLEMENTATION_PHASES.md` — MVP, Phase 2, Phase 3, and cut lines.
6. `06_PERFORMANCE_AND_ACCELERATORS.md` — fzf, FFF, fd, ripgrep, zoxide, Atuin, Television, and why each belongs or does not belong in the core.
7. `07_TMUX_IMPLEMENTATION_NOTES.md` — tmux primitives, target IDs, formats, popups, prompts, hooks, and fallback behavior.
8. `08_STATE_METADATA_REMINDERS.md` — notes, reminders, MRU history, SQLite schema, and privacy boundaries.
9. `09_LAYOUTS_AND_PROJECTS.md` — current-directory sessions, project discovery, duplicate session policy, and layouts.
10. `10_TEST_PLAN.md` — unit, integration, manual, mobile, and performance tests.
11. `11_AGENT_IMPLEMENTATION_PROMPT.md` — instructions suitable for giving to an implementation agent.
12. `config.example.toml` — proposed configuration file.
13. `tmux.example.conf` — proposed tmux bindings.

## Highest-level decision

Build a tmux-native workflow layer, not a replacement terminal multiplexer, agent runtime, dashboard, server, or app.

The tool should feel like: “tmux, but with a great switcher, current-project sessions, names, notes, layouts, and reminders.”
