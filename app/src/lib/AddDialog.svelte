<script lang="ts">
  import { open as pickFolder } from "@tauri-apps/plugin-dialog";
  import * as api from "./api";
  import { formatBytes } from "./format";
  import Icon from "./Icon.svelte";
  import FileIcon from "./FileIcon.svelte";

  let {
    defaultDir,
    segments,
    initialUrl = "",
    capture = null,
    onclose,
  }: {
    defaultDir: string;
    segments: number;
    initialUrl?: string;
    /** Set when the download comes from the browser extension. */
    capture?: api.Capture | null;
    onclose: () => void;
  } = $props();

  // Cookies and referer from the browser, so links behind a login work. They
  // only apply while the address is the one the browser sent.
  // svelte-ignore state_referenced_locally
  const browserHeaders = capture ? api.captureHeaders(capture) : [];
  // svelte-ignore state_referenced_locally
  const browserUrl = capture?.url ?? "";

  // svelte-ignore state_referenced_locally
  let url = $state(capture?.url ?? initialUrl);
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

  const headers = $derived(url.trim() === browserUrl ? browserHeaders : []);
  const validUrl = $derived(/^(https?|ftp):\/\/\S+$/i.test(url.trim()));

  /** Last path segment of the URL, as a stand-in until the server answers. */
  const urlName = $derived.by(() => {
    if (!validUrl) return "";
    try {
      const last = new URL(url.trim()).pathname.split("/").filter(Boolean).pop() ?? "";
      return decodeURIComponent(last);
    } catch {
      return "";
    }
  });
  const suggested = $derived(info?.filename || capture?.fileName || urlName);
  const shownName = $derived(fileName.trim() || suggested);

  function scheduleProbe() {
    clearTimeout(timer);
    probeSeq++;
    info = null;
    probeError = null;
    checking = false;
    if (!validUrl) return;
    checking = true;
    timer = setTimeout(runProbe, 400);
  }

  async function runProbe() {
    const seq = ++probeSeq;
    checking = true;
    try {
      const result = await api.inspectUrl(url.trim(), headers);
      if (seq !== probeSeq) return;
      info = result;
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
      await api.addDownload({ url: url.trim(), dir, fileName: fileName.trim() || undefined, headers, start });
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
    if (url) scheduleProbe();
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
    <h2>{capture ? "Download from browser" : "Add download"}</h2>

    <label class="field">
      <span>Address</span>
      <div class="input-icon">
        <Icon name="link" size={15} />
        <!-- svelte-ignore a11y_autofocus -->
        <input
          type="url"
          placeholder="https://example.com/file.zip"
          bind:value={url}
          oninput={scheduleProbe}
          autofocus
          spellcheck="false" />
      </div>
    </label>

    <div class="file" class:empty={!validUrl}>
      {#if validUrl}
        <FileIcon name={shownName || "download"} size={40} />
        <div class="file-text">
          <input
            class="name"
            type="text"
            bind:value={fileName}
            placeholder={suggested || "File name"}
            aria-label="Save as"
            title="Rename before downloading"
            spellcheck="false" />
          <div class="meta" aria-live="polite">
            {#if checking}
              <span class="muted"><span class="spin"><Icon name="loader" size={12} stroke={2.4} /></span> Checking the link…</span>
            {:else if probeError}
              <span class="warn error" title={probeError}>
                <Icon name="alert" size={12} stroke={2.4} />
                <span class="clamp">Couldn't check the link: {probeError}</span>
              </span>
            {:else if info}
              <span class="size">{info.size == null ? "Unknown size" : formatBytes(info.size)}</span>
              {#if info.resumable}
                <span class="ok"><Icon name="check" size={12} stroke={2.4} /> Resumable · up to {segments} connections</span>
              {:else}
                <span class="warn"><Icon name="alert" size={12} stroke={2.4} /> One connection, cannot resume</span>
              {/if}
            {/if}
          </div>
        </div>
      {:else}
        <Icon name="down" size={20} />
        <span>Paste a link to see the file's name and size.</span>
      {/if}
    </div>

    <div class="field">
      <span>Save to</span>
      <div class="row">
        <div class="input-icon grow">
          <Icon name="folder" size={15} />
          <input type="text" bind:value={dir} spellcheck="false" aria-label="Save to folder" />
        </div>
        <button type="button" class="btn" onclick={browse}>Browse…</button>
      </div>
    </div>

    {#if submitError}
      <p class="error-text">{submitError}</p>
    {/if}

    <div class="actions">
      <button
        type="button"
        class="btn push"
        disabled={!validUrl || busy}
        onclick={() => submit(false)}
        title="Add to the list without starting">
        <Icon name="pause" size={14} /> Add paused
      </button>
      <button type="button" class="btn" onclick={onclose}>Cancel</button>
      <button type="submit" class="btn primary" disabled={!validUrl || busy}>
        <Icon name="down" size={15} stroke={2.2} />
        {busy ? "Adding…" : "Download"}
      </button>
    </div>
  </form>
</div>

<style>
  .input-icon {
    position: relative;
    display: flex;
    align-items: center;
    color: var(--muted);
  }
  .input-icon :global(svg) {
    position: absolute;
    left: 10px;
    pointer-events: none;
  }
  .input-icon input {
    padding-left: 32px;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }

  .file {
    display: flex;
    align-items: center;
    gap: 14px;
    min-height: 72px;
    padding: 12px 14px;
    background: var(--details);
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  .file.empty {
    justify-content: center;
    gap: 8px;
    background: transparent;
    border-style: dashed;
    color: var(--muted);
  }
  .file-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  /* Looks like text until hovered or focused, then becomes a field. */
  .name {
    margin: -3px -7px;
    padding: 3px 6px;
    border: 1px solid transparent;
    border-radius: 4px;
    background: transparent;
    font-size: 14px;
    font-weight: 600;
    text-overflow: ellipsis;
  }
  .name::placeholder {
    color: var(--text);
    opacity: 1;
  }
  .name:hover {
    border-color: var(--field-border);
    background: var(--field);
  }
  .name:focus {
    border-color: var(--field-border);
    border-bottom: 2px solid var(--accent);
    background: var(--field);
    outline: none;
  }
  .name:focus::placeholder {
    color: var(--muted);
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 12px;
    min-height: 18px;
    font-size: 12px;
  }
  .meta span {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .meta .error {
    align-items: flex-start;
    min-width: 0;
  }
  .meta .error :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  /* Server errors can be long; two lines here, the rest in the tooltip. */
  .clamp {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
    overflow-wrap: anywhere;
  }
  .size {
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .muted {
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
