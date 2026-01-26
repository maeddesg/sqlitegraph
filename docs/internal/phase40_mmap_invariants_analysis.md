# Phase 40: Mmap Lifecycle Invariants Analysis

## Current State Analysis

### When GraphFile is created/opened:
- **create()**: mmap initialized AFTER header write (lines 141-147)
- **open()**: mmap initialized AFTER header read (lines 176-184)
- **Problem**: No validation that mmap covers full file size at creation

### For ANY write_bytes(offset, buf):
- **Expected**: Mapping must cover [offset, offset + buf.len())
- **Current**: May remap but doesn't guarantee write visibility in existing mmap regions
- **Problem**: Standard I/O writes vs mmap reads can be inconsistent

### When file size grows:
- **Current**: Remapping in both write_bytes() and flush_write_buffer()
- **Problem**: Multiple mmap instances can exist, causing aliasing corruption
- **Issue**: No single source of truth for file state

## Critical Issues Identified

1. **Dual Source of Truth**: File I/O and mmap are separate paths
2. **Aggressive Remapping**: Every write can trigger mmap recreation
3. **No Write Coherence**: mmap may not reflect immediately written data
4. **Header Corruption Risk**: mmap remapping can corrupt file header region

## Required Invariants for Fix

1. **Single mmap instance per GraphFile** - centralized lifecycle
2. **Write-through coherence** - all writes immediately visible in mmap
3. **Conservative remapping** - only when file grows significantly
4. **Header protection** - ensure magic bytes never corrupted by mmap operations