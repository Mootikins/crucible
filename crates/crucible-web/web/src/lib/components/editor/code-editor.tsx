import { onMount, onCleanup, createEffect, type Component } from 'solid-js'
import { EditorView, basicSetup } from 'codemirror'
import { markdown } from '@codemirror/lang-markdown'
import { javascript } from '@codemirror/lang-javascript'
import { oneDark } from '@codemirror/theme-one-dark'
import { EditorState, Compartment } from '@codemirror/state'

interface CodeEditorProps {
  value: string
  onChange?: (value: string) => void
  language?: 'markdown' | 'javascript' | 'typescript'
  theme?: 'light' | 'dark'
  class?: string
}

export const CodeEditor: Component<CodeEditorProps> = (props) => {
  let containerRef: HTMLDivElement | undefined
  let view: EditorView | undefined
  
  const languageCompartment = new Compartment()
  const themeCompartment = new Compartment()
  
  const getLanguage = () => {
    switch (props.language) {
      case 'javascript':
        return javascript()
      case 'typescript':
        return javascript({ typescript: true })
      case 'markdown':
      default:
        return markdown()
    }
  }
  
  onMount(() => {
    if (!containerRef) return
    
    const state = EditorState.create({
      doc: props.value,
      extensions: [
        basicSetup,
        languageCompartment.of(getLanguage()),
        themeCompartment.of(props.theme === 'dark' ? oneDark : []),
        EditorView.updateListener.of((update) => {
          if (update.docChanged && props.onChange) {
            props.onChange(update.state.doc.toString())
          }
        }),
        EditorView.theme({
          '&': {
            height: '100%',
            fontSize: '14px',
          },
          '.cm-scroller': {
            overflow: 'auto',
          },
        }),
      ],
    })
    
    view = new EditorView({
      state,
      parent: containerRef,
    })
  })
  
  // Update content when props.value changes externally
  createEffect(() => {
    if (view && props.value !== view.state.doc.toString()) {
      view.dispatch({
        changes: {
          from: 0,
          to: view.state.doc.length,
          insert: props.value,
        },
      })
    }
  })
  
  // Update theme when props.theme changes
  createEffect(() => {
    if (view) {
      view.dispatch({
        effects: themeCompartment.reconfigure(
          props.theme === 'dark' ? oneDark : []
        ),
      })
    }
  })
  
  // Update language when props.language changes
  createEffect(() => {
    if (view) {
      view.dispatch({
        effects: languageCompartment.reconfigure(getLanguage()),
      })
    }
  })
  
  onCleanup(() => {
    view?.destroy()
  })
  
  return (
    <div
      ref={containerRef}
      class={props.class}
      style={{ height: '100%', width: '100%' }}
    />
  )
}

