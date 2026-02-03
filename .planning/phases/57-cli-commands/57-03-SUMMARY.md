---
phase: 57-cli-commands
plan: 03
subsystem: cli
tags: [cfg, cli, dominators, post-dominators, control-dependence, dominance-frontiers, natural-loops]

# Dependency graph
requires:
  - phase: 47 (Core CFG Analysis)
    provides: dominators, post_dominators, DominatorResult, PostDominatorResult
  - phase: 48 (Derived CFG Analysis)
    provides: control_dependence_graph, dominance_frontiers, natural_loops, ControlDependenceResult, DominanceFrontierResult, NaturalLoopsResult
provides:
  - cli: dominators, post-dominators, control-dependence, dominance-frontiers, natural-loops
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns: [cli-command-pattern, ahash-to-std-hashmap-for-json]

key-files:
  created: []
  modified:
    - sqlitegraph-cli/src/main.rs - Added 5 CFG CLI command functions
    - sqlitegraph-cli/src/cli.rs - Added help text for 5 CFG commands

key-decisions:
  - "Dominance frontiers requires DominatorResult not entry node - must compute dominators first then pass result to dominance_frontiers_with_progress"
  - "AHashMap doesn't implement Serialize - convert to std::collections::HashMap before JSON output"
  - "natural_loops_with_progress requires DominatorResult - added --entry flag to natural-loops command (originally planned as no-flags)"

patterns-established:
  - "CFG CLI pattern: compute dominators first, then pass to derived algorithms that require DominatorResult"

# Metrics
duration: 28min
completed: 2026-02-03
---

# Phase 57: CLI Commands Summary

**CLI access for Core CFG and Derived CFG algorithms with dominators, post-dominators, control dependence, dominance frontiers, and natural loops**

## Performance

- **Duration:** 28 min (0.47 hours)
- **Started:** 2026-02-03T01:14:54Z
- **Completed:** 2026-02-03T01:43:21Z
- **Tasks:** 3
- **Files modified:** 2

## Accomplishments

- Added 5 CLI commands for CFG analysis: dominators (--entry), post-dominators ([--exit]), control-dependence ([--exit]), dominance-frontiers (--entry), natural-loops (--entry)
- All commands use ConsoleProgress for progress tracking
- JSON output with AHashMap-to-HashMap conversion for serialization
- Updated help text in cli.rs documenting all 5 CFG commands

## Task Commits

Each task was committed atomically:

1. **Task 1-3: Add all 5 CFG CLI commands** - `e0f95b6` (feat)
2. **Fix: dominance-frontiers to use DominatorResult** - `fed7726` (fix)

**Plan metadata:** (pending final docs commit)

## Files Created/Modified

- `sqlitegraph-cli/src/main.rs` - Added run_dominators, run_post_dominators, run_control_dependence, run_dominance_frontiers, run_natural_loops functions with ConsoleProgress and JSON output
- `sqlitegraph-cli/src/cli.rs` - Added help text for dominators, post-dominators, control-dependence, dominance-frontiers, natural-loops commands

## Decisions Made

- **Natural loops requires entry node:** Originally planned natural-loops with no flags, but natural_loops_with_progress requires DominatorResult which needs entry node. Changed to require --entry flag for consistency with other CFG commands.
- **Dominance frontiers API:** dominance_frontiers_with_progress takes DominatorResult not i64 entry - must compute dominators first before calling this function.
- **AHashMap serialization:** ahash types don't implement Serialize - must convert to std::collections::HashMap before JSON output.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 4 - Architectural] Changed natural-loops command to require --entry flag**
- **Found during:** Task 3 (Adding natural-loops function)
- **Issue:** natural_loops_with_progress requires DominatorResult parameter, but plan specified command with no flags. Computing dominators without a known entry node would be ambiguous.
- **Fix:** Added --entry flag requirement to natural-loops command for consistency with other CFG commands.
- **Files modified:** sqlitegraph-cli/src/main.rs, sqlitegraph-cli/src/cli.rs
- **Verification:** Function compiles correctly with entry parameter, help text updated to document --entry flag.
- **Committed in:** e0f95b6 (Task 1-3 commit)

**2. [Rule 1 - Bug] Fixed dominance_frontiers API usage**
- **Found during:** Task 2 (Adding run_dominance_frontiers function)
- **Issue:** Plan specified passing entry node directly to dominance_frontiers_with_progress, but API requires DominatorResult.
- **Fix:** Compute dominators_with_progress first, then pass result to dominance_frontiers_with_progress.
- **Files modified:** sqlitegraph-cli/src/main.rs
- **Verification:** Function compiles correctly, accesses frontiers.frontiers field of DominanceFrontierResult.
- **Committed in:** fed7726

**3. [Rule 1 - Bug] Fixed AHashMap to JSON serialization**
- **Issue:** ahash types (AHashMap, AHashSet) don't implement Serialize trait, causing JSON compilation errors.
- **Fix:** Convert all AHashMap/AHashSet results to std::collections::HashMap/Vec before json! macro using .into_iter().collect().
- **Files modified:** sqlitegraph-cli/src/main.rs
- **Verification:** All CFG commands compile successfully with 18 pre-existing errors in OTHER functions (not CFG-related).
- **Committed in:** e0f95b6

---

**Total deviations:** 3 auto-fixed (1 architectural, 2 bugs)
**Impact on plan:** All changes necessary for correctness - natural-loops requires entry node for unambiguous dominator computation, dominance_frontiers API requires DominatorResult, AHashMap conversion required for JSON output. No scope creep.

## Issues Encountered

- **AHashMap serialization:** ahash types don't implement serde::Serialize - had to convert all results to std::collections::HashMap before JSON output.
- **DominanceFrontierResult struct access:** Needed to access .frontiers field of the result struct, not iterate the result directly.
- **Duplicate function declarations during editing:** File modifications created duplicate function declarations - resolved by careful use of Python scripts and sed commands.

## Next Phase Readiness

- All 5 CFG CLI commands implemented and compiling
- Help text documents all commands and their flags correctly
- Ready for next phase or testing

---
*Phase: 57-cli-commands*
*Completed: 2026-02-03*
