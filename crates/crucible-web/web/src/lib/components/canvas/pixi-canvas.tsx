import { onMount, onCleanup, type Component } from 'solid-js'
import { Application, Container, Graphics, Text, TextStyle } from 'pixi.js'

interface CanvasNode {
  id: string
  x: number
  y: number
  width: number
  height: number
  label: string
  color?: number
}

interface PixiCanvasProps {
  nodes?: CanvasNode[]
  onNodeClick?: (nodeId: string) => void
  class?: string
}

export const PixiCanvas: Component<PixiCanvasProps> = (props) => {
  let containerRef: HTMLDivElement | undefined
  let app: Application | undefined
  let nodeContainer: Container | undefined
  
  const nodeGraphics = new Map<string, Graphics>()
  
  const drawNode = (node: CanvasNode) => {
    const graphics = new Graphics()
    
    // Node background
    graphics.roundRect(0, 0, node.width, node.height, 8)
    graphics.fill({ color: node.color ?? 0x374151 })
    graphics.stroke({ color: 0x6b7280, width: 1 })
    
    // Node label
    const style = new TextStyle({
      fontFamily: 'system-ui, -apple-system, sans-serif',
      fontSize: 12,
      fill: 0xffffff,
    })
    const text = new Text({ text: node.label, style })
    text.x = 8
    text.y = 8
    graphics.addChild(text)
    
    // Position
    graphics.x = node.x
    graphics.y = node.y
    
    // Interactivity
    graphics.eventMode = 'static'
    graphics.cursor = 'pointer'
    
    graphics.on('pointerdown', () => {
      props.onNodeClick?.(node.id)
    })
    
    // Drag handling
    let dragging = false
    let dragOffset = { x: 0, y: 0 }
    
    graphics.on('pointerdown', (event) => {
      dragging = true
      const position = event.global
      dragOffset = {
        x: position.x - graphics.x,
        y: position.y - graphics.y,
      }
    })
    
    graphics.on('globalpointermove', (event) => {
      if (dragging) {
        const position = event.global
        graphics.x = position.x - dragOffset.x
        graphics.y = position.y - dragOffset.y
      }
    })
    
    graphics.on('pointerup', () => {
      dragging = false
    })
    
    graphics.on('pointerupoutside', () => {
      dragging = false
    })
    
    return graphics
  }
  
  onMount(async () => {
    if (!containerRef) return
    
    app = new Application()
    
    await app.init({
      resizeTo: containerRef,
      backgroundColor: 0x1f2937,
      antialias: true,
      resolution: window.devicePixelRatio || 1,
      autoDensity: true,
    })
    
    containerRef.appendChild(app.canvas)
    
    // Create container for nodes
    nodeContainer = new Container()
    app.stage.addChild(nodeContainer)
    
    // Render initial nodes
    props.nodes?.forEach((node) => {
      const graphics = drawNode(node)
      nodeContainer!.addChild(graphics)
      nodeGraphics.set(node.id, graphics)
    })
    
    // Handle resize
    const resizeObserver = new ResizeObserver(() => {
      app?.resize()
    })
    resizeObserver.observe(containerRef)
    
    onCleanup(() => {
      resizeObserver.disconnect()
    })
  })
  
  onCleanup(() => {
    app?.destroy(true)
  })
  
  return (
    <div
      ref={containerRef}
      class={props.class}
      style={{ width: '100%', height: '100%' }}
    />
  )
}

