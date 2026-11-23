<script>
  /**
   * Who this is (design/system/components/Avatar): the contact's photo, else
   * their initials, else a person. `address` looks the contact up by number;
   * `contact` is used as given. A photo is fetched once, when the avatar
   * first scrolls into view — a long address book never asks for all of them.
   */
  import Icon from "./ui/Icon.svelte";
  import { app, view, needPhoto } from "../state.svelte.js";
  import { initials } from "../format.js";

  let { address = "", contact = null, name = "", size = "", group = false } = $props();

  const who = $derived(contact ?? (address ? view.contact(address) : null));
  const letters = $derived(group ? "" : initials(who?.name ?? name));
  const photo = $derived(who && !group ? app.photos[who.uid] || "" : "");
  let el = $state(null);

  $effect(() => {
    const uid = who?.hasPhoto && !group ? who.uid : "";
    if (!uid || !el || uid in app.photos) return;
    const io = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        io.disconnect();
        needPhoto(uid);
      }
    });
    io.observe(el);
    return () => io.disconnect();
  });
</script>

<span
  bind:this={el}
  class="ewe-avatar avatar {size ? `ewe-avatar--${size}` : ''} {letters || photo ? '' : 'ewe-avatar--neutral'}"
  aria-hidden="true"
>
  {#if photo}
    <img class="avatar__img" src={photo} alt="" draggable="false" />
  {:else if group}
    <Icon name="users" />
  {:else if letters}
    {letters}
  {:else}
    <Icon name="user" />
  {/if}
</span>
