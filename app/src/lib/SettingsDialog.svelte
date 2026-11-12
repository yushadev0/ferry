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
  // svelte-ignore state_referenced_locally
  let limitOn = $state(settings.speedLimitKib > 0);
  // svelte-ignore state_referenced_locally
  let limitKib = $state(settings.speedLimitKib || 1024);
  let error = $state<string | null>(null);

  async function browse() {
    const picked = await pickFolder({ directory: true, defaultPath: draft.downloadDir || undefined });
    if (typeof picked === "string") draft.downloadDir = picked;
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    try {
      const speedLimitKib = limitOn ? Math.max(1, Math.round(Number(limitKib) || 0)) : 0;
      onsave(await api.saveSettings({ ...draft, speedLimitKib }));
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

    <div class="scroll">
      <section>
        <h3>Downloads</h3>
        <div class="field">
          <span>Default download folder</span>
          <div class="row">
            <input
              type="text"
              bind:value={draft.downloadDir}
              spellcheck="false"
              aria-label="Default download folder" />
            <button type="button" class="btn" onclick={browse}>Browse…</button>
          </div>
        </div>

        <label class="field">
          <span>Connections per download: <b>{draft.segments}</b></span>
          <input type="range" min="1" max="16" bind:value={draft.segments} />
          <small>
            More connections are faster on most servers. If a server refuses extra connections, Ferry uses fewer on its
            own. Applies to downloads started from now on.
          </small>
        </label>
      </section>

      <section>
        <h3>Queue</h3>
        <label class="field">
          <span>Downloads at the same time: <b>{draft.maxActive}</b></span>
          <input type="range" min="1" max="10" bind:value={draft.maxActive} />
          <small>The rest wait their turn. <b>Start now</b> skips the queue.</small>
        </label>

        <label class="check">
          <input type="checkbox" bind:checked={limitOn} />
          Limit total download speed
        </label>
        {#if limitOn}
          <label class="row indent">
            <input class="num" type="number" min="1" step="64" bind:value={limitKib} aria-label="Speed limit in KB/s" />
            <span class="unit">KB/s</span>
          </label>
        {/if}
      </section>

      <section>
        <h3>Schedule</h3>
        <label class="check">
          <input type="checkbox" bind:checked={draft.scheduleEnabled} />
          Only run the queue between
        </label>
        <div class="row indent">
          <input
            type="time"
            bind:value={draft.scheduleStart}
            disabled={!draft.scheduleEnabled}
            aria-label="Queue start time" />
          <span class="unit">and</span>
          <input
            type="time"
            bind:value={draft.scheduleStop}
            disabled={!draft.scheduleEnabled}
            aria-label="Queue stop time" />
        </div>
        <small class="indent">
          Outside these hours queued downloads wait, and running ones go back to the queue with their progress kept. A
          stop time earlier than the start runs overnight.
        </small>
      </section>

      <section>
        <h3>App</h3>
        <label class="check">
          <input type="checkbox" bind:checked={draft.notifyOnComplete} />
          Notify me when a download finishes or fails
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={draft.closeToTray} />
          Keep running in the tray when the window is closed
        </label>
      </section>
    </div>

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
  .modal {
    max-height: calc(100vh - 48px);
  }
  .scroll {
    display: flex;
    flex-direction: column;
    gap: 18px;
    overflow-y: auto;
    margin: 0 -20px;
    padding: 0 20px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  h3 {
    margin: 0;
    font-size: 11.5px;
    font-weight: 650;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--muted);
  }
  small {
    color: var(--muted);
    font-size: 11.5px;
  }
  .indent {
    margin-left: 24px;
  }
  .unit {
    color: var(--muted);
  }
  input[type="range"] {
    accent-color: var(--accent);
  }
  input[type="number"],
  input[type="time"] {
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
    user-select: text;
  }
  input[type="number"] {
    width: 110px;
  }
  input:disabled {
    opacity: 0.5;
  }
  p {
    margin: 0;
  }
</style>
