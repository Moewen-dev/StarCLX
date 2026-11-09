<script lang="ts">
  // Kachel bzw. Reiter „Callback“: zeigt das Callback-Modul der Anlage
  // (<Anlage>/callback). Die Seite verbietet iframes, deshalb legt der
  // Rust-Teil ein eigenes Webview über die Fläche `.slot`. Dieses liegt
  // über allem im Fenster; solange ein Menü oder Dialog offen ist, wird es
  // ausgeblendet.
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy, onMount } from "svelte";
  import { t } from "../../i18n.svelte";

  let { server }: { server: string } = $props();

  let slot = $state<HTMLElement>();
  let error = $state("");
  /** Letzte gemeldete Lage, um doppelte Aufrufe zu sparen */
  let last = "";
  let frame = 0;

  /** Offenes Menü oder offener Dialog, den das Webview verdecken würde */
  function overlayOpen(): boolean {
    return !!document.querySelector('dialog[open], [role="dialog"], [aria-modal="true"], .scrim, .modal, .settings, .pop');
  }

  async function place() {
    frame = 0;
    if (!slot || !server) return;
    const r = slot.getBoundingClientRect();
    const visible = r.width > 0 && r.height > 0 && !document.hidden && slot.checkVisibility?.() !== false && !overlayOpen();
    const key = visible ? `${server}|${Math.round(r.left)}|${Math.round(r.top)}|${Math.round(r.width)}|${Math.round(r.height)}` : "hidden";
    if (key === last) return;
    last = key;
    try {
      if (visible) {
        await invoke("callback_show", { server, x: r.left, y: r.top, width: r.width, height: r.height });
      } else {
        await invoke("callback_hide");
      }
      error = "";
    } catch (e) {
      error = String(e);
    }
  }
  const schedule = () => {
    if (!frame) frame = requestAnimationFrame(place);
  };

  let resize: ResizeObserver | undefined;
  let mutations: MutationObserver | undefined;
  onMount(() => {
    resize = new ResizeObserver(schedule);
    if (slot) resize.observe(slot);
    resize.observe(document.body);
    // Reiterwechsel, Menüs und Dialoge ändern das DOM
    mutations = new MutationObserver(schedule);
    mutations.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ["open", "hidden", "class", "style"] });
    window.addEventListener("resize", schedule);
    document.addEventListener("visibilitychange", schedule);
    schedule();
  });
  onDestroy(() => {
    resize?.disconnect();
    mutations?.disconnect();
    window.removeEventListener("resize", schedule);
    document.removeEventListener("visibilitychange", schedule);
    if (frame) cancelAnimationFrame(frame);
    invoke("callback_hide").catch(() => {});
  });
  $effect(() => {
    void server;
    schedule();
  });

  async function reload() {
    try {
      await invoke("callback_reload", { server });
    } catch (e) {
      error = String(e);
    }
  }
  async function openWindow() {
    try {
      await invoke("callback_open", { server });
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="wrap">
  <nav>
    <span class="url">{server ? server.replace(/\/+$/, "") + "/callback" : ""}</span>
    <button onclick={reload} disabled={!server}>{t("Neu laden")}</button>
    <button onclick={openWindow} disabled={!server}>{t("In eigenem Fenster öffnen")}</button>
  </nav>
  {#if error}<p class="error">{error}</p>{/if}
  {#if server}
    <div class="slot" bind:this={slot}></div>
  {:else}
    <p class="empty">{t("Keine Anlage angemeldet.")}</p>
  {/if}
</div>

<style>
  .wrap { flex: 1; display: flex; flex-direction: column; gap: 0.5rem; height: 100%; min-height: 0; }
  nav { display: flex; align-items: center; gap: 0.4rem; }
  .url { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--muted); font-size: 0.85em; }
  .slot { flex: 1; min-height: 0; width: 100%; border-radius: 6px; background: #fff; }
  .empty { color: var(--muted); margin: 0.5rem; }
  .error { color: var(--danger, #c00); margin: 0; }
</style>
