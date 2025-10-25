# CLAP bindings

These bindings are generated from the upstream [CLAP](https://github.com/free-audio/clap) SDK.

- Commit: 69a69252fdd6ac1d06e246d9a04c0a89d9607a17
- Headers: `third_party/clap/include`

## Regeneration

1. Update the submodule: `git -C third_party/clap fetch && git -C third_party/clap checkout <commit>`.
2. Recompute the header hash:
   ```sh
   python - <<'PY'
   import hashlib
   import pathlib
   root = pathlib.Path('third_party/clap/include')
   hasher = hashlib.sha256()
   for path in sorted(root.rglob('*.h')):
       hasher.update(path.read_bytes())
   print(hasher.hexdigest())
   PY
   ```
3. Update `clap-headers.sha256` with the new hash.
4. Rebuild: `cargo build -p clap-sys` (bindgen will regenerate `bindings.rs`).
