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

## Shared worktrees

- Preserve unrelated local changes. Inspect the worktree before editing,
  staging, or committing, and keep changes scoped to the task.
