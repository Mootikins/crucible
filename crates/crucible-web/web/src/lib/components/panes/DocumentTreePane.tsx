import { createSignal, For, type Component } from 'solid-js'
import { Input } from '~/lib/components/ui/input'
import { ScrollArea } from '~/lib/components/ui/scroll-area'
import { Search, ChevronRight, ChevronDown, File, Folder } from 'lucide-solid'

interface FileNode {
  name: string
  type: 'file' | 'folder'
  path: string
  children?: FileNode[]
}

interface DocumentTreePaneProps {
  onFileSelect?: (path: string) => void
}

export const DocumentTreePane: Component<DocumentTreePaneProps> = (props) => {
  const [searchQuery, setSearchQuery] = createSignal('')
  const [expandedFolders, setExpandedFolders] = createSignal<Set<string>>(new Set(['root']))

  // Mock file structure - will be replaced with real data later
  const fileTree: FileNode[] = [
    {
      name: 'Notes',
      type: 'folder',
      path: 'notes',
      children: [
        { name: 'Project Ideas.md', type: 'file', path: 'notes/project-ideas.md' },
        { name: 'Meeting Notes.md', type: 'file', path: 'notes/meeting-notes.md' },
      ],
    },
    {
      name: 'Guides',
      type: 'folder',
      path: 'guides',
      children: [{ name: 'Getting Started.md', type: 'file', path: 'guides/getting-started.md' }],
    },
    { name: 'README.md', type: 'file', path: 'readme.md' },
  ]

  const toggleFolder = (path: string) => {
    setExpandedFolders((prev) => {
      const next = new Set(prev)
      if (next.has(path)) {
        next.delete(path)
      } else {
        next.add(path)
      }
      return next
    })
  }

  const handleFileClick = (path: string) => {
    props.onFileSelect?.(path)
  }

  const renderNode = (node: FileNode, level: number = 0) => {
    const isExpanded = expandedFolders().has(node.path)
    const indent = level * 1.5

    if (node.type === 'folder') {
      return (
        <div>
          <div
            class="flex items-center gap-1 px-2 py-1 hover:bg-accent cursor-pointer text-sm"
            style={{ 'padding-left': `${indent}rem` }}
            onClick={() => toggleFolder(node.path)}
          >
            {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
            <Folder size={14} class="text-muted-foreground" />
            <span>{node.name}</span>
          </div>
          {isExpanded && node.children && (
            <div>
              <For each={node.children}>{(child) => renderNode(child, level + 1)}</For>
            </div>
          )}
        </div>
      )
    } else {
      return (
        <div
          class="flex items-center gap-1 px-2 py-1 hover:bg-accent cursor-pointer text-sm"
          style={{ 'padding-left': `${indent}rem` }}
          onClick={() => handleFileClick(node.path)}
        >
          <File size={14} class="text-muted-foreground" />
          <span>{node.name}</span>
        </div>
      )
    }
  }

  return (
    <div class="flex flex-col h-full w-full">
      <div class="p-2 border-b border-border/30">
        <div class="relative">
          <Search class="absolute left-2 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
          <Input
            type="text"
            placeholder="Search files..."
            value={searchQuery()}
            onInput={(e) => setSearchQuery(e.currentTarget.value)}
            class="pl-8 h-8 text-sm"
          />
        </div>
      </div>
      <ScrollArea class="flex-1">
        <div class="py-1">
          <For each={fileTree}>{(node) => renderNode(node)}</For>
        </div>
      </ScrollArea>
    </div>
  )
}

