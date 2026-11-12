<script lang="ts">
  import { open as pickFolder } from "@tauri-apps/plugin-dialog";
  import * as api from "./api";

  let {
    settings,
    onsave,
    onclose,
  }: { settings: api.Settings; onsave: (s: api.Settings) => void; onclose: () => void } = $props();

  // svelte-ignore state_referenced_locally
  let draft = $state({ ...settings });
  let error = $state<string | null>(null);

  async function browse() {
    const picked = await pickFolder({ directory: true, defaultPath: draft.downloadDir || undefined });
    if (typeof picked === "string") draft.downloadDir = picked;
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    try {
      onsave(await api.saveSettings({ ...draft }));
      onclose();
    } catch (err) {
      error = api.message(err);
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <form class="modal" aria-label="Settings" onsubmit={save}>
    <h2>Settings</h2>

    <div class="field">
      <span>Default download folder</span>
      <div class="row">
        <input type="text" bind:value={draft.downloadDir} spellcheck="false" aria-label="Default download folder" />
        <button type="button" class="btn" onclick={browse}>Browse…</button>
      </div>
    </div>

    <label class="field">
      <span>Connections per download: <b>{draft.segments}</b></span>
      <input type="range" min="1" max="16" bind:value={draft.segments} />
      <small>More connections are faster on most servers. Applies to downloads started from now on.</small>
    </label>

    <label class="check">
      <input type="checkbox" bind:checked={draft.notifyOnComplete} />
      Notify me when a download finishes or fails
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={draft.closeToTray} />
      Keep running in the tray when the window is closed
    </label>

    {#if error}
      <p class="error-text">{error}</p>
    {/if}

    <div class="actions">
      <button type="button" class="btn" onclick={onclose}>Cancel</button>
      <button type="submit" class="btn primary">Save</button>
    </div>
  </form>
</div>

<style>
  small {
    color: var(--muted);
    font-size: 11.5px;
  }
  input[type="range"] {
    accent-color: var(--accent);
  }
  p {
    margin: 0;
  }
</style>
