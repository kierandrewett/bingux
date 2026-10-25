# Bingux contributor guidance

## Portability

- Do not make Bingux require files supplied only by a particular Linux
  distribution. This includes wallpapers, fonts, icons, themes, and other
  host-owned assets.
- Bundle assets that Bingux needs, use assets already shipped by a declared
  dependency, or provide a clear fallback. Optional user-selected assets may
  use host paths, but a missing optional file must not break Bingux defaults.
- Keep development and documentation capture workflows portable too. Prefer
  repository-owned fixtures and allow explicit overrides for local assets.

## Documentation

- Update user-facing documentation whenever a change affects behavior,
  defaults, configuration, commands, or troubleshooting.
- Explain configuration choices and their effects with complete, scannable
  examples. Avoid dense prose blocks and unexplained enum values.
- Use representative product screenshots. Do not use terminal output to
  narrate a screenshot or present smoke-test evidence as product documentation.

## bingux-ui first

- bingux-ui is the Bingux app SDK. App developers must never re-engineer the
  basics: menus and context menus, text editing (selection, touch handles,
  the edit bar, clipboard), inputs, keyboard focus and navigation, popups,
  sheets and panels, scrolling, tooltips, page navigation and the adaptive
  desktop and phone layouts all live in bingux-ui. They work with no setup
  and behave the same in every app, on desktop and phone.
- Before building any widget in a Bingux app (Settings, the inspector, any
  new app), check what bingux-ui already has: `packages/bingux-ui/src/controls/`,
  `rows.rs`, `group.rs`, `layout.rs`, `shell/`, `window/`, `motion.rs`, the
  tokens in `style/`, and the component list in `docs/bingux-ui.md`. Use the
  existing primitive, or extend it, before writing a new one.
- When an app needs something bingux-ui cannot do, extend bingux-ui's stable
  API instead of building a private component in the app. The extension must
  be general: another app, or a third-party developer, must plausibly need it.
  Name that second use when you add it.
- Do not add one-off edge cases or radical API changes to serve a single
  component. If no general form exists, keep the code local to the app and say
  why.
- Apps contain pages and backends only; look, layout, controls and behaviour
  come from bingux-ui and its tokens.

## Shared worktrees

- Preserve unrelated local changes. Inspect the worktree before editing,
  staging, or committing, and keep changes scoped to the task.
