# Ferry desktop app

Tauri 2 shell around [`ferry-core`](../crates/ferry-core) with a SvelteKit (SPA) + TypeScript UI.

```sh
pnpm install
pnpm tauri dev     # run with hot reload
pnpm tauri build   # NSIS / MSI installers
pnpm check         # type-check the UI
```

- `src-tauri/src/commands.rs`: commands the UI calls through `invoke`
- `src-tauri/src/lib.rs`: engine setup, tray, notifications, engine events (`ferry://event`)
- `src/routes/+page.svelte`: the download list; dialogs live in `src/lib`
- `assets/icon.svg`: icon source; regenerate with `pnpm tauri icon assets/icon.svg`
