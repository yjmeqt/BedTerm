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

```
1. /worktree-ios-dev <feature-name>   # create worktree + branch
2. Build & iterate
3. Commit & merge back to main
```

### Target

iOS 26
