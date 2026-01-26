# V1 Purge Validation Plan

## Validation Strategy Overview

This document provides exact validation steps for systematically eliminating all V1 code from the SQLiteGraph codebase and ensuring it cannot be reintroduced. The validation proceeds through 5 distinct checkpoints, each with specific compile commands, error patterns, and success criteria.

## Validation Checkpoints

### Checkpoint 1: Type System Deletion
**Objective**: Remove V1 NodeRecord/EdgeRecord types and verify no references remain

**Exact Commands**:
```bash
# Primary validation - ensure no V1 types can be compiled
cargo check --workspace --all-targets 2>&1 | tee checkpoint1_type_deletion.log

# Specific test of V1 type references
cargo test --workspace --lib --no-run 2>&1 | grep -E "(NodeRecord|EdgeRecord)" | tee v1_type_refs.log

# Verify V2-only compilation
cargo build --workspace --release 2>&1 | tee checkpoint1_v2_build.log
```

**Expected Failure Patterns** (stop and report if seen):
- `error[E0425]: cannot find value.*NodeRecord`
- `error[E0425]: cannot find value.*EdgeRecord`
- `error[E0432]: unresolved import.*NodeRecord`
- `error[E0432]: unresolved import.*EdgeRecord`
- `error[E0599]: no method named.*NodeRecord`
- `error[E0599]: no method named.*EdgeRecord`

**Success Criteria**:
- All cargo commands complete with exit code 0
- No compilation errors related to V1 types
- v1_type_refs.log is empty (0 bytes)
- Final build produces release artifacts successfully

**Error Recovery Steps**:
1. Stop immediately on any V1 type reference
2. Run `grep -r "NodeRecord\|EdgeRecord" sqlitegraph/src/ --include="*.rs"` to locate references
3. Remove or migrate each reference to V2 equivalents
4. Re-run validation until clean

### Checkpoint 2: Function Removal
**Objective**: Remove all V1 legacy functions and fallback code paths

**Exact Commands**:
```bash
# Compile test to catch V1 function references
cargo test --workspace --lib --no-run 2>&1 | tee checkpoint2_function_removal.log

# Check for legacy function patterns
cargo check --workspace 2>&1 | grep -E "(legacy|fallback|v1_|_v1)" | tee v1_function_refs.log

# Verify no V1 conditional compilation
cargo clippy --workspace --all-targets 2>&1 | grep -E "(cfg.*v1|feature.*v1)" | tee v1_conditional_refs.log

# Ensure V2-only path execution
cargo test --workspace native_backend_isolation_tests --no-run 2>&1 | tee checkpoint2_isolation_test.log
```

**Expected Failure Patterns** (stop and report if seen):
- `error[E0425]: cannot find function.*legacy_`
- `error[E0425]: cannot find function.*v1_`
- `error[E0425]: cannot find function.*_v1`
- `error[E0425]: cannot find function.*fallback_`
- `warning: dead code` for functions referencing V1 patterns
- `warning: unreachable_code` in V1 fallback paths

**Success Criteria**:
- All cargo commands complete with exit code 0
- v1_function_refs.log is empty
- v1_conditional_refs.log is empty
- No clippy warnings about V1 features or conditionals

**Error Recovery Steps**:
1. Identify V1 function calls from error logs
2. Remove entire V1 function implementations
3. Update all call sites to use V2 equivalents
4. Remove `#[cfg(feature = "v1_legacy")]` and similar conditionals
5. Re-run validation until clean

### Checkpoint 3: Test Cleanup
**Objective**: Remove all V1/parity test functions and V1 format comparison tests

**Exact Commands**:
```bash
# Compile tests to identify V1 test references
cargo test --workspace --no-run 2>&1 | tee checkpoint3_test_cleanup.log

# Find V1-specific test functions
grep -r "test.*v1\|test.*legacy\|test.*parity" sqlitegraph/tests/ --include="*.rs" | tee v1_test_functions.log

# Check for V1 format mismatch tests
grep -r "format_mismatch\|v1_format\|legacy_format" sqlitegraph/tests/ --include="*.rs" | tee v1_format_tests.log

# Verify only V2 tests remain
cargo test --workspace --list 2>&1 | grep -E "(test.*v1|test.*legacy|test.*parity)" | tee remaining_v1_tests.log
```

**Expected Failure Patterns** (stop and report if seen):
- `warning: unused import` for V1 test utilities
- `error[E0425]: cannot find value` in test modules
- `error[E0433]: failed to resolve` for V1 test imports
- `warning: function is never used` for V1 test helpers

**Success Criteria**:
- All test compilation succeeds with exit code 0
- v1_test_functions.log is empty
- v1_format_tests.log is empty
- remaining_v1_tests.log is empty
- All tests run without V1 references

**Error Recovery Steps**:
1. Delete entire V1 test files identified
2. Remove V1-specific test functions from multi-purpose test files
3. Update test imports to exclude V1 test utilities
4. Remove any test data files containing V1 format references

### Checkpoint 4: API Surface Validation
**Objective**: Update all public interfaces to remove V1 compatibility

**Exact Commands**:
```bash
# Check public API for V1 references
cargo doc --workspace --no-deps 2>&1 | tee checkpoint4_api_validation.log

# Find V1 compatibility methods in public traits
grep -r "pub.*fn.*v1\|pub.*fn.*legacy" sqlitegraph/src/ --include="*.rs" | tee v1_public_methods.log

# Check for V1 return types in public interfaces
grep -r "->.*NodeRecord\|->.*EdgeRecord" sqlitegraph/src/ --include="*.rs" | tee v1_return_types.log

# Verify V2-only public API
cargo clippy --workspace --all-targets -- -W clippy::all 2>&1 | grep -E "(v1|legacy)" | tee v1_api_warnings.log
```

**Expected Failure Patterns** (stop and report if seen):
- Documentation includes V1 references
- Public methods with V1 parameter types
- Public traits with V1 associated types
- `#[deprecated]` attributes mentioning V1 compatibility

**Success Criteria**:
- cargo doc completes without errors
- v1_public_methods.log is empty
- v1_return_types.log is empty
- v1_api_warnings.log is empty
- All public API documentation refers only to V2

**Error Recovery Steps**:
1. Remove V1 compatibility methods from public traits
2. Update public function signatures to use V2 types only
3. Remove deprecated V1 compatibility shims
4. Update all API documentation to remove V1 references

### Checkpoint 5: Final V2-Only Verification
**Objective**: Complete verification that V1 cannot be re-introduced

**Exact Commands**:
```bash
# Full workspace compilation test
cargo build --workspace --all-targets --all-features 2>&1 | tee checkpoint5_final_build.log

# Comprehensive test run to ensure functionality
cargo test --workspace --all-features 2>&1 | tee checkpoint5_final_tests.log

# Clippy with all lints enabled
cargo clippy --workspace --all-targets --all-features -- -W clippy::all -W clippy::pedantic 2>&1 | tee checkpoint5_final_clippy.log

# Audit for any remaining V1 references
grep -r -i "v1\|legacy" sqlitegraph/src/ --include="*.rs" | grep -v "// " | tee final_v1_audit.log

# Verify no V1 conditionals in source
grep -r "cfg.*v1\|feature.*v1\|!v1" sqlitegraph/src/ --include="*.rs" | tee final_conditional_audit.log

# Check lockfile for V1 dependencies
grep -i "v1\|legacy" Cargo.lock | tee lockfile_v1_audit.log
```

**Expected Failure Patterns** (stop and report if seen):
- Any compilation error
- Test failures indicating V2 functionality broken
- Clippy warnings about V1 patterns
- Non-comment V1 references in source
- V1 conditional compilation directives
- V1-specific dependencies in lockfile

**Success Criteria**:
- All commands complete with exit code 0
- final_v1_audit.log contains only comments
- final_conditional_audit.log is empty
- lockfile_v1_audit.log is empty
- Full test suite passes (≥95% test success rate)
- No clippy warnings about V1 patterns

**Error Recovery Steps**:
1. If any V1 references found, return to appropriate checkpoint
2. Fix broken V2 functionality introduced during cleanup
3. Remove any V1-specific dependencies
4. Update CI/CD configuration to prevent V1 reintroduction

## Error Pattern Recognition Guide

### Critical Compilation Errors (STOP IMMEDIATELY)
```bash
# Type not found - V1 types still referenced
error[E0425]: cannot find type `NodeRecord` in this scope
error[E0425]: cannot find type `EdgeRecord` in this scope

# Function not found - V1 functions still called
error[E0425]: cannot find function `legacy_load_nodes` in this scope
error[E0425]: cannot find function `v1_format_read` in this scope

# Import resolution - V1 modules still imported
error[E0432]: unresolved import `crate::backend::v1::types`
error[E0433]: failed to resolve: use of undeclared type or module `v1`
```

### Warning Patterns that Must be Fixed
```bash
# Dead code - unused V1 functions
warning: function is never used: `v1_compatibility_shim`

# Dead code - unused V1 imports
warning: unused import: `crate::backend::v1::NodeRecordV1`

# Unreachable code - V1 fallback paths
warning: unreachable pattern
warning: unreachable statement
```

### Dependency Issues
```bash
# Lockfile contains V1-specific crates
grep -E "(sqlitegraph-v1|legacy-v1)" Cargo.lock

# Feature flags still reference V1
grep -E '"v1"' Cargo.toml
```

## Automated Validation Scripts

### Complete Validation Script
```bash
#!/bin/bash
# v1_purge_validation.sh - Complete V1 purge validation

set -e  # Stop on any error

echo "Starting V1 purge validation..."

# Checkpoint 1: Type System Deletion
echo "Checkpoint 1: Validating type system deletion..."
cargo check --workspace --all-targets > checkpoint1_type_deletion.log 2>&1
cargo test --workspace --lib --no-run 2>&1 | grep -E "(NodeRecord|EdgeRecord)" > v1_type_refs.log || true
cargo build --workspace --release > checkpoint1_v2_build.log 2>&1

if [ -s v1_type_refs.log ]; then
    echo "FAILED: V1 type references found"
    cat v1_type_refs.log
    exit 1
fi

# Checkpoint 2: Function Removal
echo "Checkpoint 2: Validating function removal..."
cargo test --workspace --lib --no-run > checkpoint2_function_removal.log 2>&1
cargo check --workspace 2>&1 | grep -E "(legacy|fallback|v1_|_v1)" > v1_function_refs.log || true
cargo clippy --workspace --all-targets 2>&1 | grep -E "(cfg.*v1|feature.*v1)" > v1_conditional_refs.log || true

if [ -s v1_function_refs.log ] || [ -s v1_conditional_refs.log ]; then
    echo "FAILED: V1 function references found"
    cat v1_function_refs.log
    cat v1_conditional_refs.log
    exit 1
fi

# Checkpoint 3: Test Cleanup
echo "Checkpoint 3: Validating test cleanup..."
cargo test --workspace --no-run > checkpoint3_test_cleanup.log 2>&1
grep -r "test.*v1\|test.*legacy\|test.*parity" sqlitegraph/tests/ --include="*.rs" > v1_test_functions.log || true
grep -r "format_mismatch\|v1_format\|legacy_format" sqlitegraph/tests/ --include="*.rs" > v1_format_tests.log || true
cargo test --workspace --list 2>&1 | grep -E "(test.*v1|test.*legacy|test.*parity)" > remaining_v1_tests.log || true

if [ -s v1_test_functions.log ] || [ -s v1_format_tests.log ] || [ -s remaining_v1_tests.log ]; then
    echo "FAILED: V1 test references found"
    exit 1
fi

# Checkpoint 4: API Surface Validation
echo "Checkpoint 4: Validating API surface..."
cargo doc --workspace --no-deps > checkpoint4_api_validation.log 2>&1
grep -r "pub.*fn.*v1\|pub.*fn.*legacy" sqlitegraph/src/ --include="*.rs" > v1_public_methods.log || true
grep -r "->.*NodeRecord\|->.*EdgeRecord" sqlitegraph/src/ --include="*.rs" > v1_return_types.log || true
cargo clippy --workspace --all-targets 2>&1 | grep -E "(v1|legacy)" > v1_api_warnings.log || true

if [ -s v1_public_methods.log ] || [ -s v1_return_types.log ] || [ -s v1_api_warnings.log ]; then
    echo "FAILED: V1 API references found"
    exit 1
fi

# Checkpoint 5: Final V2-Only Verification
echo "Checkpoint 5: Final V2-only verification..."
cargo build --workspace --all-targets --all-features > checkpoint5_final_build.log 2>&1
cargo test --workspace --all-features > checkpoint5_final_tests.log 2>&1
cargo clippy --workspace --all-targets --all-features -- -W clippy::all -W clippy::pedantic > checkpoint5_final_clippy.log 2>&1
grep -r -i "v1\|legacy" sqlitegraph/src/ --include="*.rs" | grep -v "// " > final_v1_audit.log || true
grep -r "cfg.*v1\|feature.*v1\|!v1" sqlitegraph/src/ --include="*.rs" > final_conditional_audit.log || true
grep -i "v1\|legacy" Cargo.lock > lockfile_v1_audit.log || true

if [ -s final_v1_audit.log ] || [ -s final_conditional_audit.log ] || [ -s lockfile_v1_audit.log ]; then
    echo "FAILED: V1 references found in final audit"
    exit 1
fi

# Success summary
echo "SUCCESS: All V1 purge validation checkpoints passed!"
echo "V1 code has been completely eliminated from the codebase."

# Generate final report
cat << EOF > V1_PURGE_VALIDATION_REPORT.md
# V1 Purge Validation Report - $(date)

## Summary
All 5 validation checkpoints passed successfully. V1 code has been completely eliminated.

## Checkpoints Passed
✓ Checkpoint 1: Type System Deletion
✓ Checkpoint 2: Function Removal
✓ Checkpoint 3: Test Cleanup
✓ Checkpoint 4: API Surface Validation
✓ Checkpoint 5: Final V2-Only Verification

## Metrics
- Total source files processed: $(find sqlitegraph/src -name "*.rs" | wc -l)
- Total test files processed: $(find sqlitegraph/tests -name "*.rs" | wc -l)
- V1 references found: 0
- V1 conditional compilation directives: 0
- V1-specific dependencies: 0

## Verification Commands
All validation logs saved for audit:
- checkpoint*_*.log: Compilation and test results
- *_audit.log: Final V1 reference audits
- V1_PURGE_VALIDATION_REPORT.md: This summary report

The codebase is now V2-only and V1 code cannot be reintroduced.
EOF

echo "Validation complete. See V1_PURGE_VALIDATION_REPORT.md for summary."
```

### Continuous Integration Integration
```yaml
# .github/workflows/v1_purge_validation.yml
name: V1 Purge Validation

on:
  pull_request:
    paths:
      - 'sqlitegraph/src/**'
      - 'sqlitegraph/tests/**'
      - 'Cargo.toml'
      - 'Cargo.lock'

jobs:
  v1-purge-validation:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v3

    - name: Install Rust
      uses: actions-rs/toolchain@v1
      with:
        toolchain: stable

    - name: Run V1 Purge Validation
      run: |
        chmod +x v1_purge_validation.sh
        ./v1_purge_validation.sh

    - name: Upload Validation Logs
      uses: actions/upload-artifact@v3
      if: failure()
      with:
        name: v1-validation-logs
        path: |
          checkpoint*_*.log
          *_audit.log
```

## Success Metrics and Proof

### Quantitative Success Metrics
1. **Zero V1 References**: `grep -r -i "v1\|legacy" src/ --include="*.rs" | grep -v "// "` returns empty
2. **Zero V1 Conditionals**: `grep -r "cfg.*v1\|feature.*v1" src/ --include="*.rs"` returns empty
3. **Zero V1 Dependencies**: `grep -i "v1\|legacy" Cargo.lock` returns empty
4. **100% Compilation Success**: All cargo commands complete with exit code 0
5. **≥95% Test Success Rate**: Full test suite passes after V1 removal

### Qualitative Success Proof
1. **Clean Build Log**: No compiler errors or warnings about V1 patterns
2. **Clean Clippy Output**: No clippy warnings about dead V1 code
3. **Clean Documentation**: All generated docs reference only V2 APIs
4. **Clean Dependencies**: No V1-specific crate dependencies
5. **Maintained Functionality**: All V2 features work identically after V1 removal

### Prevention Mechanisms
1. **CI Gate**: V1 purge validation blocks any PR introducing V1 code
2. **Lint Rules**: Custom clippy lints prevent V1 patterns
3. **Audit Scripts**: Automated scanning for V1 references
4. **Documentation**: Updated API docs show only V2 interfaces
5. **Testing**: Regression tests ensure V1 removal doesn't break V2

This comprehensive validation plan ensures permanent elimination of V1 code while maintaining full V2 functionality and preventing V1 reintroduction.