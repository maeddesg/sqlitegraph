# Adding a CLI Command

**Last Updated:** 2026-02-12  
**Version:** v1.6.0

This guide explains how to add a new command to the SQLiteGraph CLI.

---

## Overview

The SQLiteGraph CLI (`sqlitegraph-cli`) uses the `clap` crate for argument parsing. Commands are defined in `sqlitegraph-cli/src/main.rs`.

### Backend Support

| Backend | CLI Flag | Status |
|---------|----------|--------|
| SQLite | `--backend sqlite` | Stable, recommended for production |
| Native V3 | `--backend native` | New in v1.6.0, recommended for development |
| Native V2 | `--backend v2` | Deprecated, removal in v1.7.0 |

---

## Step-by-Step Guide

### Step 1: Define Command Variant

Add to the `Command` enum in `sqlitegraph-cli/src/main.rs`:

```rust
#[derive(Parser, Debug)]
#[command(name = "sqlitegraph")]
#[command(about = "SQLiteGraph - Embedded Graph Database", long_about = None)]
struct Args {
    #[arg(short, long, default_value = "native")]
    backend: String,

    #[arg(short, long, default_value = "memory")]
    db: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    // ... existing commands

    /// Your command description
    YourCommand {
        /// First parameter description
        #[arg(long)]
        param1: String,

        /// Optional parameter with default
        #[arg(long, default_value = "default_value")]
        param2: String,

        /// Flag for verbose output
        #[arg(long)]
        verbose: bool,
    },
}
```

### Step 2: Implement Command Handler

Add handler function in `sqlitegraph-cli/src/main.rs`:

```rust
fn handle_your_command(args: &Args, cmd: &YourCommand) -> Result<()> {
    // Initialize backend client
    let client = BackendClient::new(args.backend.clone(), args.db.clone())?;

    if cmd.verbose {
        eprintln!("Executing YourCommand with param1={}", cmd.param1);
    }

    // Your command logic here
    let result = execute_your_command(&client, cmd.param1.clone(), cmd.param2.clone())?;

    // Output results as JSON for easy parsing
    println!("{}", serde_json::to_string_pretty(&result)?);

    Ok(())
}
```

### Step 3: Wire Up Command Match

Update the main command matching in `main()`:

```rust
fn main() -> Result<()> {
    let args = Args::parse();

    match &args.command {
        Command::Status => handle_status(&args),
        Command::List => handle_list(&args),
        // ... other commands

        // Add your command
        Command::YourCommand { param1, param2, verbose } => {
            handle_your_command(&args, &YourCommand {
                param1: param1.clone(),
                param2: param2.clone(),
                verbose: *verbose,
            })
        }
    }
}
```

### Step 4: Add BackendClient Methods (if needed)

If your command needs new backend operations, add to `sqlitegraph-cli/src/client.rs`:

```rust
impl BackendClient {
    pub fn your_operation(&self, param: &str) -> Result<String> {
        match &self.backend {
            BackendInner::Sqlite(graph) => {
                // SQLite-specific implementation
                Ok(format!("SQLite result: {}", param))
            }
            BackendInner::NativeV3(graph) => {
                // V3 backend implementation
                Ok(format!("V3 result: {}", param))
            }
            BackendInner::NativeV2(graph) => {
                // V2 backend - deprecated
                Ok(format!("V2 result: {}", param))
            }
        }
    }
}
```

### Step 5: Add Tests

Create `sqlitegraph-cli/tests/your_command_test.rs`:

```rust
use assert_cmd::Command;
use tempfile::TempDir;

#[test]
fn test_your_command_basic() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("test.db");

    Command::cargo_bin("sqlitegraph-cli")
        .unwrap()
        .args([
            "--backend", "sqlite",
            "--db", &db_path.to_string_lossy(),
            "your-command",
            "--param1", "test_value",
        ])
        .assert()
        .success();
}

#[test]
fn test_your_command_v3_backend() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("test.graph");

    Command::cargo_bin("sqlitegraph-cli")
        .unwrap()
        .args([
            "--backend", "native",
            "--db", &db_path.to_string_lossy(),
            "your-command",
            "--param1", "test_value",
        ])
        .assert()
        .success();
}

#[test]
fn test_your_command_with_verbose() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("test.db");

    Command::cargo_bin("sqlitegraph-cli")
        .unwrap()
        .args([
            "--backend", "native",
            "--db", &db_path.to_string_lossy(),
            "your-command",
            "--param1", "test_value",
            "--verbose",
        ])
        .assert()
        .success()
        .stderr(predicates::str::contains("Executing YourCommand"));
}
```

Update `sqlitegraph-cli/Cargo.toml` for test dependencies:

```toml
[dev-dependencies]
assert_cmd = "2"
predicates = "3"
tempfile = "3"
```

### Step 6: Update Documentation

Add to `MANUAL.md` Section 11 (CLI Usage):

```markdown
### Your Command

Execute your custom operation.

```bash
sqlitegraph --backend native --db mygraph.graph your-command --param1 value
```

**Options:**
- `--param1 <value>` - Required parameter description
- `--param2 <value>` - Optional parameter (default: "default_value")
- `--verbose` - Enable verbose output

**Examples:**

```bash
# Basic usage with V3 backend
sqlitegraph --db mygraph.graph your-command --param1 "test"

# With verbose output
sqlitegraph --db mygraph.graph your-command --param1 "test" --verbose

# With custom param2
sqlitegraph --db mygraph.graph your-command --param1 "test" --param2 "custom"

# Using SQLite backend
sqlitegraph --backend sqlite --db mygraph.db your-command --param1 "test"
```
```

---

## Command Templates

### Simple Query Command

For commands that just query and display data:

```rust
fn handle_query_command(args: &Args, query: &str) -> Result<()> {
    let client = BackendClient::new(args.backend.clone(), args.db.clone())?;

    let results = client.query(query)?;

    // Output as JSON
    println!("{}", serde_json::to_string_pretty(&results)?);

    Ok(())
}
```

### Bulk Operation Command

For commands that process multiple items:

```rust
fn handle_bulk_command(args: &Args, input: &str) -> Result<()> {
    let client = BackendClient::new(args.backend.clone(), args.db.clone())?;

    // Read input file
    let items: Vec<InputItem> = serde_json::from_str(&fs::read_to_string(input)?)?;

    let progress = ProgressBar::new(items.len() as u64);

    for item in items {
        client.process_item(&item)?;
        progress.inc(1);
    }

    progress.finish_with_message("Done");
    Ok(())
}
```

### Interactive Command

For commands that need user interaction:

```rust
use dialoguer::Input;

fn handle_interactive_command(args: &Args) -> Result<()> {
    let client = BackendClient::new(args.backend.clone(), args.db.clone())?;

    let name: String = Input::new()
        .with_prompt("Enter name")
        .interact_text()?;

    let result = client.create_named_item(&name)?;
    println!("Created: {:?}", result);

    Ok(())
}
```

---

## Complete Example: Adding `stats` Command

```rust
// In sqlitegraph-cli/src/main.rs

#[derive(Subcommand, Debug)]
enum Command {
    // ... existing commands

    /// Display database statistics
    Stats {
        /// Include detailed table statistics
        #[arg(long)]
        detailed: bool,
    },
}

fn handle_stats(args: &Args, detailed: bool) -> Result<()> {
    let client = BackendClient::new(args.backend.clone(), args.db.clone())?;

    let stats = client.get_statistics()?;

    println!("Database Statistics:");
    println!("  Backend: {}", stats.backend);
    println!("  Nodes: {}", stats.node_count);
    println!("  Edges: {}", stats.edge_count);
    println!("  Size: {} bytes", stats.total_size);

    if detailed {
        println!("\nDetailed Breakdown:");
        for (kind, count) in stats.kind_counts {
            println!("  {}: {}", kind, count);
        }
    }

    Ok(())
}

// In main() match:
Command::Stats { detailed } => handle_stats(&args, *detailed),
```

---

## Testing Commands

```bash
# Build and test CLI
cd sqlitegraph-cli
cargo build
cargo test

# Test your specific command
cargo run -- --backend native --db /tmp/test.graph your-command --param1 test

# Test with SQLite backend
cargo run -- --backend sqlite --db /tmp/test.db your-command --param1 test
```

---

## Best Practices

1. **Use JSON output** - Makes output parseable by scripts
2. **Support both backends** - Test with SQLite and V3
3. **Add --verbose flag** - Helps with debugging
4. **Validate inputs** - Return clear error messages
5. **Document in MANUAL.md** - Users need to know about new commands
6. **Add integration tests** - Use assert_cmd for CLI testing
7. **Handle errors gracefully** - Use `anyhow` for error handling

---

## See Also

- [MANUAL.md](../../MANUAL.md) - CLI documentation
- [Backend Architecture](../../ARCHITECTURE.md) - Backend details
- Source: `sqlitegraph-cli/src/main.rs`
