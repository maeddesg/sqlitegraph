# Adding a CLI Command

**Last Updated:** 2026-02-03
**Version:** v1.4.2

This guide explains how to add a new command to the SQLiteGraph CLI.

---

## Overview

The SQLiteGraph CLI (`sqlitegraph-cli`) uses the `clap` crate for argument parsing. Commands are defined in `sqlitegraph-cli/src/main.rs`.

---

## Step-by-Step Guide

### Step 1: Define Command Variant

Add to the `Command` enum in `sqlitegraph-cli/src/main.rs`:

```rust
#[derive(Parser, Debug)]
#[command(name = "sqlitegraph")]
#[command(about = "SQLiteGraph - Embedded Graph Database", long_about = None)]
struct Args {
    #[arg(short, long, default_value = "sqlite")]
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

    // Output results
    println!("{:?}", result);

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
            BackendInner::Native(graph) => {
                // Native-specific implementation
                Ok(format!("Native result: {}", param))
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
        .success();
}
```

Update `sqlitegraph-cli/Cargo.toml` for test dependencies:

```toml
[dev-dependencies]
assert_cmd = "2"
tempfile = "3"
```

### Step 6: Update Documentation

Add to `MANUAL.md` Section 11 (CLI Usage):

```markdown
### Your Command

Execute your custom operation.

```bash
sqlitegraph --backend sqlite --db mygraph.db your-command --param1 value
```

**Options:**
- `--param1 <value>` - Required parameter description
- `--param2 <value>` - Optional parameter (default: "default_value")
- `--verbose` - Enable verbose output

**Examples:**

```bash
# Basic usage
sqlitegraph --db mygraph.db your-command --param1 "test"

# With verbose output
sqlitegraph --db mygraph.db your-command --param1 "test" --verbose

# With custom param2
sqlitegraph --db mygraph.db your-command --param1 "test" --param2 "custom"
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

    progress.finish();

    Ok(())
}
```

### Algorithm Command

For commands that run graph algorithms:

```rust
fn handle_algorithm_command(args: &Args, algorithm: AlgorithmType) -> Result<()> {
    let client = BackendClient::new(args.backend.clone(), args.db.clone())?;

    use sqlitegraph::algo;

    let results = match algorithm {
        AlgorithmType::PageRank { damping, iterations } => {
            algo::pagerank(&client, damping, iterations)?
        }
        AlgorithmType::YourAlgorithm { param } => {
            algo::your_algorithm(&client, param)?
        }
    };

    // Output as CSV
    println!("node_id,score");
    for (node_id, score) in results {
        println!("{},{}", node_id, score);
    }

    Ok(())
}
```

---

## Command Guidelines

### DO:

1. **Support both backends** when possible
2. **Provide clear error messages** for common issues
3. **Use structured output** (JSON/CSV) for data commands
4. **Show progress** for long-running operations
5. **Validate arguments** before executing

### DON'T:

1. **Print directly to stdout** for data (use structured output)
2. **Ignore errors** - propagate them properly
3. **Hardcode paths** - accept from arguments
4. **Assume backend** - check and handle both

---

## Common Patterns

### Reading from File

```rust
#[derive(Subcommand, Debug)]
enum Command {
    Import {
        /// Input JSON file
        #[arg(long)]
        input: PathBuf,
    },
}

fn handle_import(args: &Args, input: &Path) -> Result<()> {
    let client = BackendClient::new(args.backend.clone(), args.db.clone())?;

    let data = fs::read_to_string(input)
        .map_err(|e| anyhow!("Failed to read {}: {}", input.display(), e))?;

    let items: Vec<InputItem> = serde_json::from_str(&data)
        .map_err(|e| anyhow!("Failed to parse JSON: {}", e))?;

    for item in items {
        client.import_item(item)?;
    }

    Ok(())
}
```

### Writing to File

```rust
#[derive(Subcommand, Debug)]
enum Command {
    Export {
        /// Output file path
        #[arg(long)]
        output: PathBuf,

        /// Output format (json, jsonl)
        #[arg(long, default_value = "json")]
        format: String,
    },
}

fn handle_export(args: &Args, output: &Path, format: &str) -> Result<()> {
    let client = BackendClient::new(args.backend.clone(), args.db.clone())?;

    let data = client.export_data()?;

    let output_data = match format {
        "json" => serde_json::to_string_pretty(&data)?,
        "jsonl" => {
            data.iter()
                .map(|item| serde_json::to_string(item))
                .collect::<Result<Vec<_>, _>>()?
                .join("\n")
        }
        _ => return Err(anyhow!("Unknown format: {}", format)),
    };

    fs::write(output, output_data)?;

    Ok(())
}
```

### Progress Bars

```rust
use indicatif::{ProgressBar, ProgressStyle};

fn handle_long_operation(args: &Args) -> Result<()> {
    let client = BackendClient::new(args.backend.clone(), args.db.clone())?;

    let progress = ProgressBar::new(100);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap()
    );

    for i in 0..100 {
        // Do work
        client.process_step(i)?;
        progress.inc(1);
    }

    progress.finish_with_message("Done!");

    Ok(())
}
```

---

## Testing Checklist

- [ ] Command defined in enum
- [ ] Handler function implemented
- [ ] Command wired in main()
- [ ] Tests added
- [ ] Error handling tested
- [ ] Documentation updated
- [ ] Help text works (`--help`)

---

## Common Issues

### Issue: Command not showing in --help

**Solution:** Ensure command is added to the enum and derives `Subcommand`.

### Issue: Backend client methods not available

**Solution:** Add method to `BackendClient` in `client.rs`.

### Issue: Tests fail with "command not found"

**Solution:** Ensure binary name matches `Cargo.toml` package name.

### Issue: JSON output invalid

**Solution:** Use `serde_json::to_string_pretty()` for formatted output or validate before printing.
