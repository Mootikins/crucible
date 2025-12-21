import { type Component, type JSX, splitProps } from 'solid-js'
import { Dialog as KobalteDialog } from '@kobalte/core/dialog'
import { cn } from '~/lib/utils'

const Dialog = KobalteDialog

const DialogTrigger = KobalteDialog.Trigger

const DialogPortal = KobalteDialog.Portal

const DialogClose = KobalteDialog.CloseButton

const DialogOverlay: Component<KobalteDialog.OverlayProps> = (props) => {
  const [local, others] = splitProps(props, ['class'])
  return (
    <KobalteDialog.Overlay
      class={cn(
        'fixed inset-0 z-50 bg-black/80 data-[expanded]:animate-in data-[closed]:animate-out data-[closed]:fade-out-0 data-[expanded]:fade-in-0',
        local.class
      )}
      {...others}
    />
  )
}

const DialogContent: Component<KobalteDialog.ContentProps> = (props) => {
  const [local, others] = splitProps(props, ['class', 'children'])
  return (
    <DialogPortal>
      <DialogOverlay />
      <KobalteDialog.Content
        class={cn(
          'fixed left-1/2 top-1/2 z-50 grid w-full max-w-lg -translate-x-1/2 -translate-y-1/2 gap-4 border bg-background p-6 shadow-lg duration-200 data-[expanded]:animate-in data-[closed]:animate-out data-[closed]:fade-out-0 data-[expanded]:fade-in-0 data-[closed]:zoom-out-95 data-[expanded]:zoom-in-95 data-[closed]:slide-out-to-left-1/2 data-[closed]:slide-out-to-top-[48%] data-[expanded]:slide-in-from-left-1/2 data-[expanded]:slide-in-from-top-[48%] sm:rounded-lg',
          local.class
        )}
        {...others}
      >
        {local.children}
        <DialogClose class="absolute right-4 top-4 rounded-sm opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:pointer-events-none data-[expanded]:bg-accent data-[expanded]:text-muted-foreground">
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
            class="h-4 w-4"
          >
            <path d="M18 6 6 18" />
            <path d="m6 6 12 12" />
          </svg>
          <span class="sr-only">Close</span>
        </DialogClose>
      </KobalteDialog.Content>
    </DialogPortal>
  )
}

const DialogHeader: Component<JSX.HTMLAttributes<HTMLDivElement>> = (props) => {
  const [local, others] = splitProps(props, ['class'])
  return (
    <div
      class={cn('flex flex-col space-y-1.5 text-center sm:text-left', local.class)}
      {...others}
    />
  )
}

const DialogFooter: Component<JSX.HTMLAttributes<HTMLDivElement>> = (props) => {
  const [local, others] = splitProps(props, ['class'])
  return (
    <div
      class={cn('flex flex-col-reverse sm:flex-row sm:justify-end sm:space-x-2', local.class)}
      {...others}
    />
  )
}

const DialogTitle: Component<KobalteDialog.TitleProps> = (props) => {
  const [local, others] = splitProps(props, ['class'])
  return (
    <KobalteDialog.Title
      class={cn('text-lg font-semibold leading-none tracking-tight', local.class)}
      {...others}
    />
  )
}

const DialogDescription: Component<KobalteDialog.DescriptionProps> = (props) => {
  const [local, others] = splitProps(props, ['class'])
  return (
    <KobalteDialog.Description
      class={cn('text-sm text-muted-foreground', local.class)}
      {...others}
    />
  )
}

export {
  Dialog,
  DialogPortal,
  DialogOverlay,
  DialogClose,
  DialogTrigger,
  DialogContent,
  DialogHeader,
  DialogFooter,
  DialogTitle,
  DialogDescription,
}

