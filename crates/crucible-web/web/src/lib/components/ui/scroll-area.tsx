import { type Component, type JSX, splitProps } from 'solid-js'
import { cn } from '~/lib/utils'

export interface ScrollAreaProps extends JSX.HTMLAttributes<HTMLDivElement> {}

export function ScrollArea(props: ScrollAreaProps) {
  const [local, others] = splitProps(props, ['class', 'children'])
  return (
    <div
      class={cn('relative overflow-auto', local.class)}
      {...others}
    >
      {local.children}
    </div>
  )
}

