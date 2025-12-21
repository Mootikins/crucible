// Simple markdown renderer
// For production, consider using a library like marked or markdown-it

export function renderMarkdown(markdown: string): string {
  if (!markdown) return ''

  let html = markdown

  // Headers
  html = html.replace(/^### (.*$)/gim, '<h3>$1</h3>')
  html = html.replace(/^## (.*$)/gim, '<h2>$1</h2>')
  html = html.replace(/^# (.*$)/gim, '<h1>$1</h1>')

  // Bold
  html = html.replace(/\*\*(.*?)\*\*/gim, '<strong>$1</strong>')
  html = html.replace(/__(.*?)__/gim, '<strong>$1</strong>')

  // Italic
  html = html.replace(/\*(.*?)\*/gim, '<em>$1</em>')
  html = html.replace(/_(.*?)_/gim, '<em>$1</em>')

  // Code blocks
  html = html.replace(/```([\s\S]*?)```/gim, '<pre><code>$1</code></pre>')

  // Inline code
  html = html.replace(/`([^`]+)`/gim, '<code>$1</code>')

  // Links
  html = html.replace(/\[([^\]]+)\]\(([^)]+)\)/gim, '<a href="$2">$1</a>')

  // Line breaks
  html = html.replace(/\n\n/gim, '</p><p>')
  html = html.replace(/\n/gim, '<br>')

  // Wrap in paragraph if not already wrapped
  if (!html.startsWith('<')) {
    html = '<p>' + html + '</p>'
  }

  // Lists (simple unordered)
  html = html.replace(/^\- (.*$)/gim, '<li>$1</li>')
  html = html.replace(/(<li>.*<\/li>)/s, '<ul>$1</ul>')

  return html
}

