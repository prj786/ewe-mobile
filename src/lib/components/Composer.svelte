<script>
  /**
   * The message box: a field that grows with the text (up to six lines) and
   * a send button. Enter sends, Shift+Enter makes a new line. Once a text
   * no longer fits one SMS it says how many it becomes — a long Georgian
   * message is several, and the carrier bills each.
   */
  import { tick } from "svelte";
  import Icon from "./ui/Icon.svelte";
  import { smsParts } from "../format.js";

  let { placeholder = "Message", disabled = false, send = async () => true, autofocus = false } = $props();

  let text = $state("");
  let busy = $state(false);
  let field = $state(null);

  const parts = $derived(smsParts(text));
  const empty = $derived(!text.trim());

  function fit() {
    if (!field) return;
    field.style.height = "auto";
    field.style.height = `${field.scrollHeight}px`;
  }

  async function submit() {
    if (empty || busy || disabled) return;
    const body = text;
    busy = true;
    // the box empties at once; a refused send gives the words back
    text = "";
    await tick();
    fit();
    let ok = false;
    try {
      ok = await send(body);
    } finally {
      busy = false;
    }
    if (!ok && !text) {
      text = body;
      await tick();
      fit();
    }
    field?.focus();
  }

  function onKey(e) {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      submit();
    }
  }

  export function focus() {
    field?.focus();
  }

  $effect(() => {
    if (autofocus && field) field.focus();
  });
</script>

<form
  class="composer"
  onsubmit={(e) => {
    e.preventDefault();
    submit();
  }}
>
  <label class="ewe-input composer__field">
    <textarea
      bind:this={field}
      bind:value={text}
      rows="1"
      {placeholder}
      {disabled}
      aria-label={placeholder}
      oninput={fit}
      onkeydown={onKey}
    ></textarea>
  </label>
  <button
    type="submit"
    class="ewe-iconbtn ewe-iconbtn--primary ewe-iconbtn--lg"
    title="Send"
    aria-label="Send"
    disabled={empty || busy || disabled}
  >
    <Icon name="send" />
  </button>
  {#if parts.parts > 1}
    <span class="composer__count" aria-live="polite">Sends as {parts.parts} messages</span>
  {/if}
</form>
