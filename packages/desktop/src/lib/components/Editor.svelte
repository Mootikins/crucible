<script lang="ts">
  // Basic editor component
  let content = '';
</script>

<div class="editor">
  <textarea bind:value={content} placeholder="Start typing..."></textarea>
</div>

<style>
  .editor {
    flex: 1;
    padding: 1rem;
  }
  
  textarea {
    width: 100%;
    height: 100%;
    border: none;
    outline: none;
    resize: none;
    font-family: inherit;
    font-size: 1rem;
    line-height: 1.5;
  }
</style>
