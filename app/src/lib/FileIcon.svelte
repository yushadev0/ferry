<script lang="ts" module>
  import { fileIcon } from "./api";

  // One request per extension for the whole session.
  const cache = new Map<string, Promise<string | null>>();

  function systemIcon(ext: string): Promise<string | null> {
    if (!ext) return Promise.resolve(null);
    let hit = cache.get(ext);
    if (!hit) {
      hit = fileIcon(ext).catch(() => null);
      cache.set(ext, hit);
    }
    return hit;
  }

  type Kind = "archive" | "disc" | "video" | "audio" | "image" | "pdf" | "app" | "text" | "file";

  const KINDS: Record<string, Kind> = {};
  for (const [kind, exts] of Object.entries({
    archive: "zip rar 7z tar gz tgz xz bz2 zst",
    disc: "iso img dmg vhd vhdx",
    video: "mp4 mkv avi mov webm m4v wmv",
    audio: "mp3 flac wav m4a ogg opus aac",
    image: "png jpg jpeg gif webp svg bmp heic avif",
    pdf: "pdf",
    app: "exe msi msix appx deb rpm apk appimage",
    text: "txt md json csv xml log yaml yml",
  })) {
    for (const ext of exts.split(" ")) KINDS[ext] = kind as Kind;
  }
</script>

<script lang="ts">
  import { extension } from "./format";

  let { name, size = 32 }: { name: string; size?: number } = $props();

  const ext = $derived(extension(name));
  const kind = $derived<Kind>(KINDS[ext] ?? "file");
  let src = $state<string | null>(null);

  $effect(() => {
    const wanted = ext;
    src = null;
    systemIcon(wanted).then((url) => {
      if (wanted === ext) src = url;
    });
  });
</script>

{#if src}
  <img {src} width={size} height={size} alt="" draggable="false" />
{:else}
  <!-- Drawn fallback, used until (or when) the system has no icon. -->
  <svg width={size} height={size} viewBox="0 0 32 32" aria-hidden="true">
    <path d="M8 2.5h11.5L26 9v19.5a1 1 0 0 1-1 1H8a1 1 0 0 1-1-1v-25a1 1 0 0 1 1-1z" fill="#FFFFFF" stroke="#B8BFC9" />
    <path d="M19.5 2.5V9H26" fill="#EEF1F5" stroke="#B8BFC9" stroke-linejoin="round" />
    {#if kind === "archive"}
      <rect x="10" y="12" width="13" height="14" rx="1.5" fill="#E8A317" />
      <path d="M16.5 12v3M15 14.5h3M16.5 16v2M15 18.5h3" stroke="#6B4A00" stroke-width="1.1" />
      <rect x="14.5" y="20" width="4" height="4" rx="1" fill="#6B4A00" />
    {:else if kind === "disc"}
      <circle cx="16.5" cy="19.5" r="6.5" fill="#8C96A6" />
      <circle cx="16.5" cy="19.5" r="4.6" fill="#B9C2CF" />
      <circle cx="16.5" cy="19.5" r="1.8" fill="#FFFFFF" />
    {:else if kind === "video"}
      <rect x="9.5" y="13" width="14" height="11" rx="2" fill="#7C4DDB" />
      <path d="M15 15.8v5.4l4.4-2.7z" fill="#FFFFFF" />
    {:else if kind === "audio"}
      <circle cx="14" cy="22" r="3" fill="#D6336C" />
      <path d="M17 22v-9l5-1.5v8" fill="none" stroke="#D6336C" stroke-width="2" />
      <circle cx="20" cy="20" r="2.2" fill="#D6336C" />
    {:else if kind === "image"}
      <rect x="9.5" y="13" width="14" height="11" rx="1.5" fill="#2F9E44" />
      <path d="M10.5 23l4-4.5 3 3 2-2 3 3.5z" fill="#FFFFFF" />
      <circle cx="19.5" cy="16.2" r="1.4" fill="#FFFFFF" />
    {:else if kind === "pdf"}
      <rect x="7" y="13" width="15" height="7.5" rx="1.5" fill="#D93025" />
      <path d="M10 23h12M10 25.5h9" stroke="#C9CED6" stroke-width="1.2" stroke-linecap="round" />
    {:else if kind === "app"}
      <rect x="9.5" y="13" width="14" height="12" rx="1.5" fill="#0F6CBD" />
      <rect x="9.5" y="13" width="14" height="3" rx="1.5" fill="#0A4F8C" />
      <rect x="11.5" y="18" width="6" height="5" rx=".8" fill="#FFFFFF" />
    {:else if kind === "text"}
      <path d="M10 14h12M10 17h12M10 20h12M10 23h8" stroke="#8A939E" stroke-width="1.4" stroke-linecap="round" />
    {:else}
      <rect x="10" y="13" width="13" height="13" rx="2" fill="#6B7684" />
    {/if}
  </svg>
{/if}

<style>
  img,
  svg {
    flex: none;
    display: block;
  }
</style>
