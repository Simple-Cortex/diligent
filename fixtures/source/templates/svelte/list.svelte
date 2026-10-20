<script>
  export let photos = [];
</script>

<ul>
  {#each photos as p}
    <li><img src={p.url} alt={p.caption}></li>
  {/each}
</ul>
{#if photos.length === 0}
  <p id="empty">No photos</p>
{:else}
  <p id="empty">Photos below</p>
{/if}
<Banner title="x" />
<img src="/no-alt.png">
