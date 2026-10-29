To keep a copy of the source `.espf` file in `.espforge/`, you can write it in **`espforge-core/src/project.rs`** right alongside where `src/app.rs` is copied.

---

### 1. Where to Implement It

In `espforge-core`, `Source` holds the full original text of the ESPF document in `source.text`. `espforge-core::project::write_checked` is the exact place where `source` is written out to disk.

Open **`espforge-core/src/project.rs`** and locate the end of `write_checked`:

```rust
    // The user's app logic is copied verbatim on every build.
    std::fs::create_dir_all(out.join("src"))?;
    std::fs::write(out.join("src").join("app.rs"), app)?;

    Ok(Written {
        crate_name: checked.meta.crate_name(),
        notes: checked.notes.clone(),
    })
```

Add the copy of `source.text` into `.espforge/`:

```rust
    // The user's app logic is copied verbatim on every build.
    std::fs::create_dir_all(out.join("src"))?;
    std::fs::write(out.join("src").join("app.rs"), app)?;

    // Preserve the ESPF source that produced this build in .espforge/
    std::fs::create_dir_all(out.join(".espforge"))?;
    std::fs::write(out.join(".espforge").join("source.espf"), &source.text)?;

    Ok(Written {
        crate_name: checked.meta.crate_name(),
        notes: checked.notes.clone(),
    })
```

> **Note on filename:** Using `.espforge/source.espf` is standard, or `.espforge/source.espf.txt` if you prefer the `.txt` extension used by the embedded examples. 
> 
> If you want to retain the original filename when generated from a file, you can do:
> ```rust
>     let espf_filename = match &source.example {
>         Some(_) => "example.espf.txt".to_string(),
>         None => std::path::Path::new(&source.name)
>             .file_name()
>             .and_then(|n| n.to_str())
>             .unwrap_or("source.espf")
>             .to_string(),
>     };
>     std::fs::create_dir_all(out.join(".espforge"))?;
>     std::fs::write(out.join(".espforge").join(espf_filename), &source.text)?;
> ```
> However, a fixed name like `.espforge/source.espf` is usually easier for tools and scripts to inspect predictably.

---

### 2. Update Tests

Update the file assertion checks in the test suites so CI passes:

#### In `espforge-core/src/project.rs`:
```rust
        for f in [
            "Cargo.toml",
            "build.rs",
            "src/lib.rs",
            "src/bin/main.rs",
            "src/app.rs",
            "README.txt",
            ".espforge/manifest.json",
            ".espforge/source.espf", // <--- Add this
        ] {
            assert!(out.join(f).is_file(), "missing {f}");
        }
```

#### In `espforge/src/commands/generate.rs`:
```rust
        for f in [
            "Cargo.toml",
            "build.rs",
            "src/lib.rs",
            "src/bin/main.rs",
            "src/app.rs",
            "README.txt",
            ".espforge/manifest.json",
            ".espforge/source.espf", // <--- Add this
        ] {
            assert!(out.join(f).is_file(), "missing {f}");
        }
```

Now, every time `espforge gen` or `espforge build` runs, `.espforge/source.espf` will store the exact ESPF file that produced the firmware build.
