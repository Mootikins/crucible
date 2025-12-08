<script lang="ts">
  import Chat from '$lib/Chat.svelte';
</script>

<svelte:head>
  <title>Crucible Chat</title>
  <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.10.0/styles/github-dark.min.css" />
</svelte:head>

<main>
  <header>
    <h1>Crucible Chat</h1>
  </header>
  <Chat />
</main>

<style>
  :global(body) {
    margin: 0;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
    background: #fafafa;
  }

  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  header {
    background: #1976d2;
    color: white;
    padding: 0.75rem 1rem;
    text-align: center;
  }

  h1 {
    margin: 0;
    font-size: 1.25rem;
    font-weight: 500;
  }
</style>
