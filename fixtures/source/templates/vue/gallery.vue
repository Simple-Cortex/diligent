<script setup>
defineProps(['photos', 'hero', 'ok', 'moreLabel'])
</script>

<template>
  <section>
    <h2>Gallery</h2>
    <ul>
      <li v-for="p in photos" :key="p.id">
        <img :src="p.url" :alt="p.caption">
      </li>
    </ul>
    <img :src="hero" alt="">
    <img src="/x.png">
    <FancyButton v-if="ok" label="Go" />
    <a v-else href="/more"><span>{{ moreLabel }}</span></a>
  </section>
</template>
