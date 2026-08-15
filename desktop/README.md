# Desktop shell

The React/TypeScript frontend, Tauri 2 Rust shell, SQL plugin setup, and first SQLite migration are present. `npm run build` and the default `cargo check -p wow-coach-desktop` require no generated assets.

Tauri's native context requires platform icon binaries. This baseline intentionally does not commit them. To launch locally:

```sh
cd desktop
npm ci
npm run tauri icon src-tauri/icons/icon.svg
npm run tauri dev -- --features desktop-runtime
```

Generated PNG/ICO/ICNS files under `src-tauri/icons/` are ignored and must not be committed. Release bundling and signing remain roadmap work.