<script lang="ts">
  let { size = 24 }: { size?: number } = $props();
</script>

<!-- The life ring app icon (see app/assets/icon.svg). -->
<svg width={size} height={size} viewBox="0 0 64 64" aria-hidden="true">
  <circle cx="32" cy="32" r="22.5" fill="none" stroke="#FFFFFF" stroke-width="12" />
  <circle
    cx="32"
    cy="32"
    r="22.5"
    fill="none"
    stroke="#E2483D"
    stroke-width="12"
    stroke-dasharray="17.67 17.67"
    transform="rotate(-11.25 32 32)" />
  <circle cx="32" cy="32" r="28.5" fill="none" stroke="#1B3A57" stroke-width="1.6" />
  <circle cx="32" cy="32" r="16.5" fill="#EAF2FC" stroke="#1B3A57" stroke-width="1.6" />
  <path
    d="M32 23v15M25.5 32 32 38.5 38.5 32"
    fill="none"
    stroke="#1F6FD1"
    stroke-width="4"
    stroke-linecap="round"
    stroke-linejoin="round" />
</svg>
