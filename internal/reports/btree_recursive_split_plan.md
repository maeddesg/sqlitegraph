# BTree Recursive Split Implementation Plan

## Research Summary

Based on research from CMU Database course, StackOverflow discussions, and PostgreSQL implementation notes.

## The Problem

Current V3 BTree implementation fails when:
1. Leaf node splits
2. Parent (internal) node is also full
3. Need to recursively split up to root

Current code throws: `"parent page full - recursive split not yet implemented"`

## Two Main Approaches

### Approach 1: Post-Split (Standard Algorithm)
Split nodes bottom-up after insertion attempt fails.

**Algorithm:**
```
insert(key, value):
    path = traverse_to_leaf(key)  // Track all ancestors
    leaf = path.pop()
    
    if leaf has space:
        insert into leaf
        return
    
    // Split leaf
    (new_leaf, middle_key) = split_leaf(leaf)
    
    // Propagate up
    current = leaf
    split_key = middle_key
    
    while path not empty:
        parent = path.pop()
        
        if parent has space:
            insert split_key and new child pointer into parent
            return
        
        // Split parent too
        (new_parent, new_split_key) = split_internal(parent)
        insert split_key into appropriate side
        current = parent
        split_key = new_split_key
    
    // If we get here, root was split - create new root
    create_new_root(current, split_key, new_node)
```

**Pros:**
- Standard textbook algorithm
- Well understood

**Cons:**
- Need parent pointers OR path tracking
- May require multiple tree traversals if re-traversing

### Approach 2: Pre-emptive Splitting (Recommended by Wikipedia)
Split full nodes during descent, before insertion.

**Algorithm:**
```
insert(key, value):
    if root is full:
        split_root()
    
    current = root
    while current is not leaf:
        // Split any full child we encounter
        next_child = find_child_for_key(current, key)
        
        if next_child is full:
            split_child(current, next_child)
            // After split, re-determine which child to go to
            next_child = find_child_for_key(current, key)
        
        current = next_child
    
    // Now current is leaf with guaranteed space
    insert_into_leaf(current, key, value)
```

**Pros:**
- Single pass down the tree
- No need for parent pointers or complex path tracking
- Child is guaranteed to have space after parent's split

**Cons:**
- May split nodes unnecessarily (conservative)
- Requires U = 2L constraint (max children = 2 × min children)

**Requirements:**
- Max keys must be even (splits evenly)
- This is why we changed MAX_KEYS from 254 to 253

## Recommended Approach: Pre-emptive Splitting

### Why Pre-emptive for SQLiteGraph V3:
1. **Simpler implementation** - no path tracking needed
2. **Single pass** - better cache locality
3. **Already used by PostgreSQL** - proven in production
4. **Matches our constraints** - we can enforce even MAX_KEYS

### Implementation Steps

#### Step 1: Update Constants
```rust
// Current
pub const MAX_KEYS: usize = 253;     // Should be even for preemptive
pub const MAX_ENTRIES: usize = 253;

// Change to even numbers
pub const MAX_KEYS: usize = 252;     // Even - splits into 126/126
pub const MAX_ENTRIES: usize = 252;  // Even - splits into 126/126
pub const MIN_KEYS: usize = 126;     // ceil(MAX_KEYS/2)
```

#### Step 2: Implement `split_child()` for Internal Nodes
```rust
fn split_child(&mut self, parent: &mut IndexPage, child_idx: usize) -> NativeResult<()> {
    // 1. Get child page
    let child_page_id = parent.children[child_idx];
    let mut child = self.load_page(child_page_id)?;
    
    // 2. Create new node
    let new_page_id = self.allocator.allocate()?;
    let mut new_child = child.clone_empty(new_page_id);
    
    // 3. Split keys/children
    let mid = child.keys.len() / 2;
    let mid_key = child.keys[mid];
    
    // Move right half to new node
    new_child.keys = child.keys.split_off(mid + 1);
    new_child.children = child.children.split_off(mid + 1);
    
    // Remove middle key from child (it goes to parent)
    child.keys.pop(); // Remove the last key (was mid_key)
    
    // 4. Insert into parent
    parent.keys.insert(child_idx, mid_key);
    parent.children.insert(child_idx + 1, new_page_id);
    
    // 5. Write pages
    self.write_page(&child)?;
    self.write_page(&new_child)?;
    self.write_page(parent)?;
    
    Ok(())
}
```

#### Step 3: Implement Preemptive Insert
```rust
pub fn insert_preemptive(&mut self, key: i64, value: u64) -> NativeResult<()> {
    // Handle empty tree
    if self.root_page_id == EMPTY_TREE_ROOT {
        return self.insert_into_empty_tree(key, value);
    }
    
    // Check if root needs splitting
    let root = self.load_page(self.root_page_id)?;
    if root.is_full() {
        self.split_root()?;
    }
    
    // Descend and split as needed
    self.insert_non_full(self.root_page_id, key, value)
}

fn insert_non_full(&mut self, page_id: u64, key: u64, value: u64) -> NativeResult<()> {
    let mut page = self.load_page(page_id)?;
    
    if page.is_leaf() {
        // Insert into leaf
        return self.insert_into_leaf(&mut page, key, value);
    }
    
    // Find child to descend to
    let child_idx = page.find_child_index(key);
    let child_id = page.children[child_idx];
    
    // Check if child is full
    let child = self.load_page(child_id)?;
    if child.is_full() {
        // Split child first
        self.split_child(&mut page, child_idx)?;
        
        // After split, re-determine which child to use
        let child_idx = page.find_child_index(key);
        let child_id = page.children[child_idx];
    }
    
    // Recurse into child
    self.insert_non_full(child_id, key, value)
}
```

#### Step 4: Implement Split Root
```rust
fn split_root(&mut self) -> NativeResult<()> {
    let old_root_id = self.root_page_id;
    let old_root = self.load_page(old_root_id)?;
    
    // Create new root (internal node)
    let new_root_id = self.allocator.allocate()?;
    let mut new_root = IndexPage::new_internal(new_root_id);
    
    // Create sibling for old root
    let sibling_id = self.allocator.allocate()?;
    let mut sibling = old_root.clone_empty(sibling_id);
    
    // Split keys/children
    let mid = old_root.keys.len() / 2;
    let mid_key = old_root.keys[mid];
    
    // Move right half to sibling
    sibling.keys = old_root.keys.split_off(mid + 1);
    if old_root.is_internal() {
        sibling.children = old_root.children.split_off(mid + 1);
    }
    
    // Remove middle key from old root
    old_root.keys.pop();
    
    // Set up new root
    new_root.keys.push(mid_key);
    new_root.children.push(old_root_id);
    new_root.children.push(sibling_id);
    
    // Write all pages
    self.write_page(&old_root)?;
    self.write_page(&sibling)?;
    self.write_page(&new_root)?;
    
    // Update root
    self.root_page_id = new_root_id;
    self.tree_height += 1;
    
    Ok(())
}
```

## Key Design Decisions

### 1. When to Split
- **Preemptive:** Split full children during descent
- **Benefit:** Guaranteed space at leaf, no backtracking

### 2. What to Promote
- Internal split: Middle key goes to parent
- Leaf split: First key of new leaf goes to parent (for B+Tree)

### 3. Key Distribution
- Left node: keys[0..mid]  
- Parent gets: keys[mid]
- Right node: keys[mid+1..]

### 4. B+Tree Specifics
- Leaf nodes linked (next_leaf pointer)
- Only leaf nodes store actual values
- Internal nodes store keys + child pointers

## Testing Strategy

### Unit Tests
1. **Split Leaf:** Insert until split, verify structure
2. **Split Internal:** Trigger internal node split
3. **Split Root:** Insert enough to split root
4. **Cascading Split:** Insert sequence causing multiple splits

### Integration Tests
1. **Sequential Insert:** 1, 2, 3, ... N
2. **Random Insert:** Random order
3. **Reverse Insert:** N, N-1, ... 1
4. **Large Dataset:** 100k+ inserts

### Validation
- Tree invariants (min/max keys)
- All values retrievable
- No duplicate keys
- Height balanced

## Risk Assessment

| Risk | Mitigation |
|------|-----------|
| Split logic bugs | Extensive unit tests |
| Page ID collisions | Shared Arc<RwLock<PageAllocator>> |
| Corruption on crash | WAL integration for splits |
| Performance regression | Benchmark before/after |

## Next Steps

1. **Decision:** Confirm preemptive splitting approach
2. **Implementation:** Split into 3 PRs:
   - PR 1: Update constants + split_child()
   - PR 2: insert_non_full() + split_root()
   - PR 3: Integration + tests
3. **Testing:** Run stress tests up to 1M nodes
4. **Documentation:** Update V3 architecture docs

## References

1. [CMU Database Course - B+Tree Project](https://15445.courses.cs.cmu.edu/spring2023/project2/)
2. [StackOverflow - BTree Parent Pointers](https://stackoverflow.com/questions/74595458/should-b-tree-nodes-contain-a-pointer-to-their-parent-c-implementation)
3. [Rahul Soni - B+Tree Implementation](https://rahul-soni.com/posts/implementing-bplustree/)
4. [PostgreSQL BTree README](https://github.com/postgres/postgres/blob/master/src/backend/access/nbtree/README)
5. [Wikipedia - B-tree Insertion](https://en.wikipedia.org/wiki/B-tree#Insertion)
