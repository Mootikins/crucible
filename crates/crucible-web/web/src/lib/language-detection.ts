const EXT_TO_LANG: Record<string, string> = {
  rs: 'rust',
  ts: 'typescript',
  tsx: 'tsx',
  js: 'javascript',
  jsx: 'jsx',
  mjs: 'javascript',
  cjs: 'javascript',
  py: 'python',
  go: 'go',
  json: 'json',
  yaml: 'yaml',
  yml: 'yaml',
  toml: 'toml',
  md: 'markdown',
  markdown: 'markdown',
  html: 'html',
  htm: 'html',
  css: 'css',
  sh: 'sh',
  bash: 'bash',
  sql: 'sql',
};

export function languageFromFileName(fileName: string | undefined): string {
  if (!fileName) return 'text';
  const base = fileName.split('/').pop() ?? fileName;
  const dot = base.lastIndexOf('.');
  if (dot < 0 || dot === base.length - 1) return 'text';
  const ext = base.slice(dot + 1).toLowerCase();
  return EXT_TO_LANG[ext] ?? 'text';
}
