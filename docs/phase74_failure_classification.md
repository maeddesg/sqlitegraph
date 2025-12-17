# Phase 74 Failure Classification

## Evidence Captured

### Serialization and Write Traces
```
[phase74] SERIALIZE: node_id=813, direction="serialize", size=8, checksum32=0x672ed1d2, first_32=[00, 00, 00, 01, 00, 00, 00, 1c], last_32=[00, 00, 00, 01, 00, 00, 00, 1c]
[phase74] SERIALIZE_FINAL: edges=1, size=36, checksum32=0x72f62146, first_32=[00, 00, 00, 01, 00, 00, 00, 1c, 00, 00, 00, 00, 00, 00, 03, 2d, 00, 7d, 00, 10, 7b, 22, 65, 64, 67, 65, 5f, 69, 6e, 64, 65, 78], last_32=[00, 00, 00, 1c, 00, 00, 00, 00, 00, 00, 03, 2d, 00, 7d, 00, 10, 7b, 22, 65, 64, 67, 65, 5f, 69, 6e, 64, 65, 78, 22, 3a, 30, 7d]
[phase74] WRITE_PRE: tx_id=1, node_id=776, direction=Outgoing, checksum32=0x72f62146, size=36
[phase74] SERIALIZE: node_id=776, direction="serialize", size=8, checksum32=0x672ed1d2, first_32=[00, 00, 00, 01, 00, 00, 00, 1c], last_32=[00, 00, 00, 01, 00, 00, 00, 1c]
[phase74] SERIALIZE_FINAL: edges=1, size=36, checksum32=0x952f4bab, first_32=[00, 00, 00, 01, 00, 00, 00, 1c, 00, 00, 00, 00, 00, 00, 03, 08, 00, 7d, 00, 10, 7b, 22, 65, 64, 67, 65, 5f, 69, 6e, 64, 65, 78], last_32=[00, 00, 00, 1c, 00, 00, 00, 00, 00, 00, 03, 08, 00, 7d, 00, 10, 7b, 22, 65, 64, 67, 65, 5f, 69, 6e, 64, 65, 78, 22, 3a, 30, 7d]
[phase74] WRITE_PRE: tx_id=1, node_id=813, direction=Incoming, checksum32=0x952f4bab, size=36
```

### Error Observed
```
Error: ConnectionError("Inconsistent adjacency for node 776: outgoing 1 != 0 in file")
PHASE 72: rollback_floor = 4097024, final_rollback_size = 4097024
PHASE 72: Transaction rolled back to offset 4097024
```

## Pattern Analysis

1. **Serialization Working**: Clusters are being serialized correctly with proper checksums and sizes
   - Node 776 outgoing cluster: checksum32=0x72f62146, size=36
   - Node 813 incoming cluster: checksum32=0x952f4bab, size=36
   - Both have valid framed format (edges=1, proper headers)

2. **Writes Initiated**: WRITE_PRE traces show clusters being written to disk
   - tx_id=1 indicates Phase 70 atomic transaction is active
   - Both outgoing and incoming clusters being written for the same edge

3. **Rollback Triggered**: After writes, transaction rollback occurs
   - "PHASE 72: Transaction rolled back to offset 4097024"
   - This suggests Phase 72 rollback logic is being triggered

4. **Metadata Mismatch**: After rollback, adjacency check fails
   - "outgoing 1 != 0 in file" means node metadata claims 1 edge but actual cluster data shows 0 edges

## Classification: BUCKET M (Metadata mismatch)

**Evidence**:
- Cluster serialization is working correctly (proper checksums, sizes, headers)
- Clusters are being written successfully
- Error occurs after transaction rollback
- Node metadata (adjacency count) doesn't match actual cluster content

**Root Cause**: The cluster data is being written correctly, but the transaction rollback is not properly updating or rolling back the node metadata that references those clusters. The node metadata still points to clusters that may have been truncated during rollback.

**First divergence point**: The inconsistency occurs at `sqlitegraph/src/backend/native/graph_file.rs:350` in the rollback transaction logic where cluster data is truncated but node metadata referencing those clusters is not properly updated.