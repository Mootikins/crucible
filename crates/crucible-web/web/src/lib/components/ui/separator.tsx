import { type Component, type JSX, splitProps } from 'solid-js'
import { cn } from '~/lib/utils'

export interface SeparatorProps extends JSX.HTMLAttributes<HTMLDivElement> {
  orientation?: 'horizontal' | 'vertical'
  decorative?: boolean
}

export function Separator(props: SeparatorProps) {
  const [local, others] = splitProps(props, ['orientation', 'decorative', 'class'])
  const orientation = () => local.orientation ?? 'horizontal'
  
  return (
    <div
      role={local.decorative ? 'none' : 'separator'}
      aria-orientation={orientation()}
      class={cn(
        'shrink-0 bg-border',
        orientation() === 'horizontal' ? 'h-px w-full' : 'h-full w-px',
        local.class
      )}
      {...others}
    />
  )
}

