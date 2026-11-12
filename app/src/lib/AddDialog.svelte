<script lang="ts">
  import { open as pickFolder } from "@tauri-apps/plugin-dialog";
  import * as api from "./api";
  import { formatBytes } from "./format";

  let {
    defaultDir,
    initialUrl = "",
    onclose,
  }: { defaultDir: string; initialUrl?: string; onclose: () => void } = $props();

  // svelte-ignore state_referenced_locally
  let url = $state(initialUrl);
  // svelte-ignore state_referenced_locally
  let dir = $state(defaultDir);
  let fileName = $state("");
  let info = $state<api.ProbeInfo | null>(null);
  let checking = $state(false);
  let probeError = $state<string | null>(null);
  let submitError = $state<string | null>(null);
  let busy = $state(false);

  let probeSeq = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const validUrl = $derived(/^(https?|ftp):\/\/\S+$/i.test(url.trim()));

  function scheduleProbe() {
    clearTimeout(timer);
    info = null;
    probeError = null;
    if (!validUrl) return;
    timer = setTimeout(runProbe, 400);
  }

  async function runProbe() {
    const seq = ++probeSeq;
    checking = true;
    try {
      const result = await api.inspectUrl(url.trim());
      if (seq !== probeSeq) return;
      info = result;
      if (!fileName && result.filename) fileName = result.filename;
    } catch (e) {
      if (seq === probeSeq) probeError = api.message(e);
    } finally {
      if (seq === probeSeq) checking = false;
    }
  }

  async function browse() {
    const picked = await pickFolder({ directory: true, defaultPath: dir || undefined });
    if (typeof picked === "string") dir = picked;
  }

  async function submit(start: boolean) {
    if (!validUrl || busy) return;
    busy = true;
    submitError = null;
    try {
      await api.addDownload({ url: url.trim(), dir, fileName: fileName.trim() || undefined, start });
      onclose();
    } catch (e) {
      submitError = api.message(e);
    } finally {
      busy = false;
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }

  $effect(() => {
    if (initialUrl) scheduleProbe();
    return () => clearTimeout(timer);
  });
</script>

<svelte:window {onkeydown} />

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <form
    class="modal"
    aria-label="Add download"
    onsubmit={(e) => {
      e.preventDefault();
      submit(true);
    }}>
    <h2>Add download</h2>

    <label class="field">
      <span>Address</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        type="url"
        placeholder="https://example.com/file.zip"
        bind:value={url}
        oninput={scheduleProbe}
        autofocus
        spellcheck="false" />
    </label>

    <div class="probe" aria-live="polite">
      {#if checking}
        <span class="muted">Checking the link…</span>
      {:else if probeError}
        <span class="error-text">{probeError}</span>
      {:else if info}
        <span><b>{formatBytes(info.size)}</b></span>
        <span class="dot">·</span>
        <span class={info.resumable ? "ok" : "warn"}>
          {info.resumable ? "Resumable, segmented" : "Single connection, cannot resume"}
        </span>
      {:else}
        <span class="muted">Paste a link to see the file size.</span>
      {/if}
    </div>

    <label class="field">
      <span>Save as</span>
      <input type="text" bind:value={fileName} placeholder="Name suggested by the server" spellcheck="false" />
    </label>

    <div class="field">
      <span>Folder</span>
      <div class="row">
        <input type="text" bind:value={dir} spellcheck="false" aria-label="Folder" />
        <button type="button" class="btn" onclick={browse}>Browse…</button>
      </div>
    </div>

    {#if submitError}
      <p class="error-text">{submitError}</p>
    {/if}

    <div class="actions">
      <button type="button" class="btn" onclick={onclose}>Cancel</button>
      <button type="button" class="btn" disabled={!validUrl || busy} onclick={() => submit(false)}>
        Add paused
      </button>
      <button type="submit" class="btn primary" disabled={!validUrl || busy}>
        {busy ? "Adding…" : "Download"}
      </button>
    </div>
  </form>
</div>

<style>
  .probe {
    display: flex;
    gap: 6px;
    align-items: center;
    min-height: 20px;
    font-size: 12px;
  }
  .muted,
  .dot {
    color: var(--muted);
  }
  .ok {
    color: var(--ok);
  }
  .warn {
    color: var(--warn);
  }
  p {
    margin: 0;
  }
</style>
