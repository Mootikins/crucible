import { type Component, splitProps } from 'solid-js'
import ResizablePrimitive from '@corvu/resizable'
import { cn } from '~/lib/utils'

const ResizablePanelGroup: Component<ResizablePrimitive.RootProps> = (props) => {
  const [local, others] = splitProps(props, ['class'])
  return (
    <ResizablePrimitive
      class={cn('flex h-full w-full data-[orientation=vertical]:flex-col', local.class)}
      {...others}
    />
  )
}

const ResizablePanel = ResizablePrimitive.Panel

const ResizableHandle: Component<ResizablePrimitive.HandleProps> = (props) => {
  const [local, others] = splitProps(props, ['class'])
  return (
    <ResizablePrimitive.Handle
      class={cn(
        'relative flex w-px items-center justify-center bg-border after:absolute after:inset-y-0 after:left-1/2 after:w-1 after:-translate-x-1/2 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring focus-visible:ring-offset-1 data-[orientation=vertical]:h-px data-[orientation=vertical]:w-full data-[orientation=vertical]:after:left-0 data-[orientation=vertical]:after:h-1 data-[orientation=vertical]:after:w-full data-[orientation=vertical]:after:-translate-y-1/2 data-[orientation=vertical]:after:translate-x-0 [&[data-orientation=vertical]>div]:rotate-90',
        local.class
      )}
      {...others}
    >
      <div class="z-10 flex h-4 w-3 items-center justify-center rounded-sm border bg-border">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="24"
          height="24"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          class="h-2.5 w-2.5"
        >
          <circle cx="12" cy="5" r="1" />
          <circle cx="12" cy="12" r="1" />
          <circle cx="12" cy="19" r="1" />
        </svg>
      </div>
    </ResizablePrimitive.Handle>
  )
}

export { ResizablePanelGroup, ResizablePanel, ResizableHandle }

