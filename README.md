# BedTerm

iOS terminal for AI-assisted coding. SSH into a dev box and run Claude Code / Codex CLI from iPhone.

## Development

Worktree-based branching model. Each feature gets its own git worktree.

### Skills

| Skill | What it does |
|-------|--------------|
| `/prd` | Load the product requirements document |
| `/worktree-ios-dev` | iOS build, test, run, simulator management within a worktree |

### Workflow

Worktrees live at `.worktrees/`.

```
1. git worktree add .worktrees/<feature-name> -b feature/<feature-name>
2. Build & iterate with /worktree-ios-dev
3. Commit & merge back to main
```

### Target

iOS 26

## Build & test

```sh
brew install mint
mint bootstrap

xcodebuild test \
  -project BedTerm.xcodeproj \
  -scheme BedTerm \
  -destination 'platform=iOS Simulator,name=iPhone 16 Pro' \
  | mint run xcbeautify
```

## Lint

```sh
mint run swiftlint lint --strict
mint run swiftformat --lint .
```

