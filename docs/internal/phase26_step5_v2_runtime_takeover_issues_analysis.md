# Phase 26 Step 5 - V2 Runtime Takeover Issues Analysis

## Critical Blocking Issues

### 1. V2 Header Parsing Corruption During Edge Operations

**Files:**
- `sqlitegraph/src/backend/native/node_store.rs` (lines 801-809)
- `sqlitegraph/src/backend/native/v2/node_record_v2/record.rs` (line 406+)

**Methods:**
- `NodeStore::read_node_v2()`
- `parse_v2_header_lengths()`

**Issue:**
- **Error:** `CorruptNodeRecord { node_id: 1, reason: "Node record truncated: need 65589 bytes, have 8192" }`
- **Debug Output:** `node 1 header lengths kind=0 name=0 data=65536 remaining=8192`
- **Root Cause:** Bytes 17-20 being interpreted as `[1, 0, 0, 0]` (65536) instead of correct data length
- **Expected:** Small data length (should be < 50 bytes for `json!({})`)
- **Actual:** 65536 bytes requested

**Analysis:**
- V2 nodes are being written correctly (writer takeover successful)
- V2 parsing works in isolation (direct V2 tests pass)
- Issue occurs only during EdgeStore adjacency operations
- Suggests file offset or buffer corruption during EdgeStore → NodeStore interaction

### 2. EdgeStore V2 Integration Failure

**Files:**
- `sqlitegraph/src/backend/native/edge_store.rs` (lines 125-137)
- `sqlitegraph/src/backend/native/edge_store.rs` (lines 85-87)

**Methods:**
- `EdgeStore::update_node_adjacency_v2()`
- `EdgeStore::update_node_adjacency()`

**Issue:**
- EdgeStore correctly routes to V2 method under `v2_experimental` feature
- V2 method calls `NodeStore::read_node_v2()` which fails with header corruption
- Edge operations trigger V2 node reading but receive corrupted header data
- Results in edge write failure and test failure

### 3. Inconsistent V1/V2 Routing During Mixed Operations

**Files:**
- `sqlitegraph/src/backend/native/node_store.rs` (lines 46-50, 254-262)
- `sqlitegraph/tests/v2_takeover_routing_tests.rs` (lines 101-104)

**Methods:**
- `NodeStore::write_node()` - has V2 routing via conversion
- `NodeStore::read_node()` - has version detection
- Test methods using V1 NodeRecord with V2 write routing

**Issue:**
- Test creates V1 NodeRecord objects
- `write_node()` converts to V2 and writes correctly
- EdgeStore tries to read V2 nodes during edge operations
- Version detection or V2 parsing fails during integrated operations

## Non-Critical Issues

### 1. Warning: Unreachable Code

**File:** `sqlitegraph/src/backend/native/node_store.rs`
**Lines:** 53 (unreachable statement after return)
**Method:** `NodeStore::write_node()`
**Issue:** Code after `return self.write_node_v2(&v2_record);` is unreachable under v2_experimental

### 2. Warning: Unused Imports

**Files:** Multiple files throughout codebase
**Examples:**
- `sqlitegraph/src/backend/native/adjacency.rs` line 29: `NodeRecordV2` unused
- `sqlitegraph/src/backend/native/edge_store.rs` line 11: `NodeRecordV2Ext` unused
- Multiple other unused import warnings

## Architecture Issues

### 1. Mixed V1/V2 Data Formats in Same Runtime

**Files:** Multiple
**Problem:** Under `v2_experimental`, system writes V2 records but may have legacy V1 parsing code paths active

**Impact:** Creates potential for format corruption and inconsistent behavior

### 2. EdgeStore Assumes V1 Format During Node Reading

**File:** `sqlitegraph/src/backend/native/edge_store.rs`
**Methods:** Various methods that interact with NodeStore
**Issue:** EdgeStore may not consistently use V2-aware reading methods

## Test Infrastructure Issues

### 1. Test Uses V1 NodeRecord with V2 Runtime

**File:** `sqlitegraph/tests/v2_takeover_routing_tests.rs`
**Lines:** 101-104
**Method:** `adjacency_uses_clustered_metadata_by_default()`
**Issue:** Test creates V1 NodeRecord objects expecting V2 runtime behavior
**Current Behavior:** V1→V2 conversion works, but EdgeStore reading fails

## Recommendations for Resolution

### Immediate (Critical Path)
1. **Debug V2 Header Corruption**: Add detailed hex dump of header bytes in `read_node_v2`
2. **Verify File Offsets**: Ensure V2 nodes written at correct offsets
3. **Test EdgeStore → NodeStore Integration**: Create isolated test for this interaction

### Medium Priority
1. **Fix Unreachable Code Warnings**: Reorganize V2 routing logic
2. **Clean Up Unused Imports**: Remove or conditionally import V2-specific code
3. **Standardize Test Data**: Use V2 NodeRecord in V2 tests or create V1-V2 compatibility tests

### Long-term Architecture
1. **Complete V2 Runtime**: Ensure all paths use V2 consistently under feature flag
2. **V1/V2 Compatibility Layer**: Create clear separation between formats
3. **Comprehensive Integration Testing**: Test all combinations of V1/V2 operations

## Files Needing Investigation

1. **sqlitegraph/src/backend/native/node_store.rs**
   - `read_node_v2()` method (line 773+)
   - `parse_v2_header_lengths()` call site (line 803)
   - V2 header reading logic

2. **sqlitegraph/src/backend/native/v2/node_record_v2/record.rs**
   - `parse_v2_header_lengths()` function (line 406+)
   - V2 header format expectations

3. **sqlitegraph/src/backend/native/edge_store.rs**
   - `update_node_adjacency_v2()` method (line 125+)
   - NodeStore interaction patterns

4. **sqlitegraph/tests/v2_takeover_routing_tests.rs**
   - `adjacency_uses_clustered_metadata_by_default()` test
   - V1→V2 compatibility expectations