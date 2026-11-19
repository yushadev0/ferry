<!-- The small window for downloads sent by the browser: only the Add dialog,
     one download at a time, closed when none are left. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
  import * as api from "$lib/api";
  import AddDialog from "$lib/AddDialog.svelte";
  import { setLanguage, t } from "$lib/i18n.svelte";

  const win = getCurrentWindow();

  let settings = $state<api.Settings | null>(null);
  /** Browser downloads still waiting, oldest first. */
  const waiting: api.Capture[] = [];
  let capture = $state<api.Capture | null>(null);
  let content: HTMLElement | undefined = $state();
  let shown = false;

  async function pull() {
    waiting.push(...(await api.takeCaptures()));
    if (!capture) next();
  }

  function next() {
    capture = waiting.shift() ?? null;
    if (!capture) win.close();
  }

  // The window takes the dialog's height and appears once it has it.
  $effect(() => {
    if (!content) return;
    const observer = new ResizeObserver(async () => {
      const height = Math.ceil(content!.getBoundingClientRect().height);
      if (!height) return;
      await win.setSize(new LogicalSize(520, height));
      if (!shown) {
        shown = true;
        await win.show();
        await win.setFocus();
      }
    });
    observer.observe(content);
    return () => observer.disconnect();
  });

  onMount(() => {
    let unlisten: (() => void) | undefined;
    (async () => {
      settings = await api.getSettings();
      api.applyTheme(settings.theme);
      setLanguage(settings.language);
      await win.setTitle(t("add.fromBrowser"));
      unlisten = await api.onCapture(pull);
      await pull();
    })().catch(() => win.close());
    return () => unlisten?.();
  });
</script>

{#if settings && capture}
  <div bind:this={content}>
    {#key capture}
      <AddDialog
        defaultDir={settings.downloadDir}
        segments={settings.segments}
        {capture}
        standalone
        onclose={next} />
    {/key}
  </div>
{/if}

<style>
  :global(html),
  :global(body) {
    background: var(--panel);
  }
</style>
