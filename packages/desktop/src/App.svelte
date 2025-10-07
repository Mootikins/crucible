<script lang="ts">
  import { onMount } from 'svelte';
  import { documentStore } from './lib/stores/document';
  import Editor from './lib/components/Editor.svelte';
  import PanelLayout from './lib/components/PanelLayout.svelte';
  import { initializeDatabase } from './lib/db';
  
  let initialized = false;
  
  onMount(async () => {
    await initializeDatabase();
    initialized = true;
  });
</script>

{#if initialized}
  <PanelLayout>
    <Editor />
  </PanelLayout>
{:else}
  <div class="loading">
    🔥 Initializing Crucible...
  </div>
{/if}

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    font-family: system-ui, -apple-system, sans-serif;
  }
  
  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    font-size: 1.5rem;
    color: #666;
  }
</style>

