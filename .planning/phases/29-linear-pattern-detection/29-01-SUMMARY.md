---
phase: 29-linear-pattern-detection
plan: 01
subsystem: instrumentation
tags: [linear-detection, state-machine, pattern-detection, fsm, adjacency]

# Dependency graph
requires:
  - phase: 28-performance-validation
    provides: Validated per-traversal cache and MVCC isolation patterns
  - phase: 26-bfs-traversal-cache
    provides: TraversalCache and AdjacencyHelpers::outgoing_degree() for O(1) degree checks
provides:
  - LinearDetector state machine with 4-state FSM (Unknown, Accumulating, Linear, Branching)
  - TraversalPattern enum for classification (Unknown, Linear, Branching)
  - observe() method for step-by-step degree observation
  - confidence() method returning 0.0-1.0 based on state
  - reset() method for per-traversal reuse
affects:
  - phase: 30-sequential-read-buffer
    needs: LinearDetector to trigger sequential I/O optimization
  - phase: 31-traversal-integration
    needs: LinearDetector integration into traversal hot paths

# Tech tracking
tech-stack:
  added: []
  patterns:
    - 4-state FSM for pattern detection (Unknown -> Accumulating -> Linear OR Branching)
    - Per-traversal detector state (not global/thread-local) to preserve MVCC isolation
    - 3-step threshold prevents tree false positives
    - Confidence score based on progress toward threshold

key-files:
  created:
    - sqlitegraph/src/backend/native/adjacency/linear_detector.rs
  modified:
    - sqlitegraph/src/backend/native/adjacency/mod.rs

key-decisions:
  - "3-step threshold prevents tree false positives (trees have 1-2 linear segments before branching)"
  - "Per-traversal detector (not global) preserves MVCC isolation - detector evaporates when function returns"
  - "Read-only instrumentation in Phase 29, no I/O behavior changes until Phase 31 integration"
  - "O(1) degree check via AdjacencyHelpers::outgoing_degree() - no full neighbor fetch"

patterns-established:
  - "Pattern 1: 4-state FSM (Unknown -> Accumulating -> Linear OR Branching) for linear pattern detection"
  - "Pattern 2: Per-traversal detector state, reset() for reuse, no global state"
  - "Pattern 3: Confidence score 0.0-1.0 based on consecutive_linear/threshold progress"

# Metrics
duration: 3min
completed: 2026-01-21
---

# Phase 29: Plan 01 - LinearDetector State Machine Summary

**4-state finite state machine detecting linear traversal patterns (degree <= 1 for 3+ consecutive steps)**

## Performance

- **Duration:** 3 min
- **Started:** 2026-01-21T10:44:32Z
- **Completed:** 2026-01-21T10:47:00Z
- **Tasks:** 1
- **Files created:** 1
- **Files modified:** 1

## Accomplishments

- Created `LinearDetector` state machine with 4-state FSM (Unknown, Accumulating, Linear, Branching)
- Created `TraversalPattern` enum with Unknown, Linear, Branching variants
- Implemented `observe()` method for step-by-step degree observation during traversal
- Implemented `confidence()` method returning 0.0-1.0 based on state and progress
- Implemented `reset()` method for per-traversal reuse
- Added comprehensive unit tests (13 tests, all passing)
- Exported `LinearDetector` and `TraversalPattern` from adjacency module

## Task Commits

Each task was committed atomically:

1. **Task 1: Create LinearDetector state machine with TraversalPattern enum** - `7df0ad4` (feat)

**Plan metadata:** (to be added after this commit)

## Files Created/Modified

- `sqlitegraph/src/backend/native/adjacency/linear_detector.rs` - NEW (650 lines)
  - `TraversalPattern` enum: Unknown, Linear, Branching (Copy, Clone, Debug, PartialEq, Eq)
  - `DetectorState` enum (private): Unknown, Accumulating, Linear, Branching
  - `LinearDetector` struct: state, consecutive_linear, threshold
  - Public methods: new(), with_threshold(), observe(), confidence(), reset(), current_pattern(), is_linear_confirmed()
  - Default impl for LinearDetector
  - 13 comprehensive unit tests

- `sqlitegraph/src/backend/native/adjacency/mod.rs` - MODIFIED
  - Added `mod linear_detector;`
  - Added `pub use linear_detector::{LinearDetector, TraversalPattern};`

## Test Results

**LinearDetector unit tests:** 13 passed
- `test_linear_detector_new` - Initial state verification
- `test_linear_detector_default` - Default impl verification
- `test_linear_detector_with_threshold` - Custom threshold creation
- `test_linear_detector_chain_confirms_after_three` - Chain graph confirms Linear after 3 steps
- `test_linear_detector_star_immediate_branching` - Star graph immediately detects Branching
- `test_linear_detector_diamond_transitions_to_branching` - Diamond transitions Unknown -> Branching
- `test_linear_detector_linear_then_branching` - Linear -> Branching transition
- `test_linear_detector_dead_end_stays_unknown` - Degree 0 stays in Unknown
- `test_linear_detector_reset` - Reset clears state correctly
- `test_linear_detector_custom_threshold` - Custom threshold behavior
- `test_linear_detector_confidence_progression` - Confidence score progression
- `test_linear_detector_current_pattern` - Current pattern accessor
- `test_traversal_pattern_traits` - Type trait verification

## Decisions Made

- **3-step threshold**: Prevents false positives on tree graphs which have 1-2 linear segments before branching
  - Rationale: STATE.md v1.4 research confirmed trees rarely have 3+ consecutive degree-1 steps without branching
- **Per-traversal detector design**: Detector is created per-traversal, not stored globally
  - Rationale: Preserves MVCC isolation, prevents cross-traversal state leakage
- **O(1) degree check**: Detector observes degree passed as parameter, doesn't fetch neighbors
  - Rationale: Performance - full neighbor fetch is expensive, degree check via AdjacencyHelpers is O(1)
- **Read-only instrumentation**: Phase 29 adds detection only, no I/O behavior changes
  - Rationale: Validate detection accuracy before modifying I/O behavior in Phase 31

## Deviations from Plan

### Auto-fixed Issues

None - plan executed exactly as written.

---

**Total deviations:** 0
**Impact on plan:** None

## Issues Encountered

None - implementation followed specification exactly from RESEARCH.md.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- LinearDetector state machine implemented and tested (13 tests passing)
- All public methods verified: new(), with_threshold(), observe(), confidence(), reset(), current_pattern(), is_linear_confirmed()
- State transitions match RESEARCH.md specification exactly
- Ready for Phase 30 (SequentialReadBuffer implementation)

**Requirements satisfied:**
- Created LinearDetector with 4-state FSM (Unknown, Accumulating, Linear, Branching)
- TraversalPattern enum with Unknown, Linear, Branching variants
- observe() method with degree parameter
- confidence() method returning 0.0-1.0
- reset() method for new traversals
- All tests passing (13/13)

---
*Phase: 29-linear-pattern-detection, Plan: 01*
*Completed: 2026-01-21*
